import * as cdk from 'aws-cdk-lib/core';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as dynamodb from 'aws-cdk-lib/aws-dynamodb';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as ecsPatterns from 'aws-cdk-lib/aws-ecs-patterns';
import * as elbv2 from 'aws-cdk-lib/aws-elasticloadbalancingv2';
import * as route53 from 'aws-cdk-lib/aws-route53';
import * as route53Targets from 'aws-cdk-lib/aws-route53-targets';
import * as s3 from 'aws-cdk-lib/aws-s3';
import { Construct } from 'constructs';

export interface ScorekeeperStackProps extends cdk.StackProps {
  /**
   * The environment name (dev, staging, prod)
   * @default 'dev'
   */
  readonly environmentName?: string;

  /**
   * The desired count of Fargate tasks
   * @default 1 for dev, 2 for prod
   */
  readonly desiredCount?: number;

  /**
   * The name of the S3 bucket for avatar uploads
   * @default 'scorekeeper-avatars'
   */
  readonly avatarBucketName?: string;

  /**
   * Whether to import an existing S3 avatar bucket instead of creating a new one
   * @default false
   */
  readonly importExistingAvatarBucket?: boolean;
}

export class ScorekeeperStack extends cdk.Stack {
  public readonly loadBalancerDnsName: cdk.CfnOutput;
  public readonly ecrRepositoryUri: cdk.CfnOutput;

  constructor(scope: Construct, id: string, props?: ScorekeeperStackProps) {
    super(scope, id, props);

    const environmentName = props?.environmentName ?? 'dev';
    const isProd = environmentName === 'prod';
    const desiredCount = props?.desiredCount ?? (isProd ? 2 : 1);

    // Look up the existing ECR Repository created by PrerequisiteInfraStack
    const ecrRepository = ecr.Repository.fromRepositoryName(
      this,
      'ScorekeeperRepository',
      'scorekeeper'
    );

    // S3 bucket for avatar uploads
    const avatarBucketName = props?.avatarBucketName ?? 'scorekeeper-avatars';

    // Either import an existing bucket or create a new one
    const avatarBucket = props?.importExistingAvatarBucket
      ? s3.Bucket.fromBucketName(this, 'AvatarBucket', avatarBucketName)
      : new s3.Bucket(this, 'AvatarBucket', {
          bucketName: avatarBucketName,
          blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
          encryption: s3.BucketEncryption.S3_MANAGED,
          enforceSSL: true,
          removalPolicy: cdk.RemovalPolicy.RETAIN,
          cors: [
            {
              allowedMethods: [s3.HttpMethods.PUT, s3.HttpMethods.GET],
              allowedOrigins: ['*'], // Restricted by IAM and pre-signed URLs
              allowedHeaders: ['*'],
              maxAge: 3600,
            },
          ],
        });

    // Route 53 Hosted Zone for the API subdomain (NS records configured in Namecheap)
    const domainName = this.node.tryGetContext('domainName') ?? 'wordles.dev';
    const apiSubdomain = this.node.tryGetContext('apiSubdomain') ?? 'api';
    const apiDomainName = `${apiSubdomain}.${domainName}`;

    // Enable HTTPS only after DNS is configured (staged deployment)
    // Stage 1: Deploy with enableHttps=false to get hosted zone nameservers
    // Stage 2: Configure NS records in Namecheap, wait for propagation
    // Stage 3: Deploy with enableHttps=true to create certificate and enable HTTPS
    const enableHttps = this.node.tryGetContext('enableHttps') === 'true';

    const hostedZone = new route53.HostedZone(this, 'HostedZone', {
      zoneName: apiDomainName,
    });

    // ACM Certificate for the API subdomain with DNS validation (only when HTTPS enabled)
    const certificate = enableHttps
      ? new acm.Certificate(this, 'ApiCertificate', {
          domainName: apiDomainName,
          validation: acm.CertificateValidation.fromDns(hostedZone),
        })
      : undefined;

    // DynamoDB table for game data (single-table design)
    const table = new dynamodb.Table(this, 'ScorekeeperTable', {
      tableName: `scorekeeper-${environmentName}`,
      partitionKey: { name: 'pk', type: dynamodb.AttributeType.STRING },
      sortKey: { name: 'sk', type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      pointInTimeRecovery: isProd,
    });

    // GSI for querying games by game_id (leaderboard queries)
    table.addGlobalSecondaryIndex({
      indexName: 'GameSessionIndex',
      partitionKey: { name: 'game_id', type: dynamodb.AttributeType.STRING },
      projectionType: dynamodb.ProjectionType.ALL,
    });

    // Create VPC - using a simple 2-AZ VPC setup
    const vpc = new ec2.Vpc(this, 'ScorekeeperVpc', {
      vpcName: `scorekeeper-vpc-${environmentName}`,
      maxAzs: 2,
      natGateways: isProd ? 2 : 1,
      subnetConfiguration: [
        {
          name: 'Public',
          subnetType: ec2.SubnetType.PUBLIC,
          cidrMask: 24,
        },
        {
          name: 'Private',
          subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS,
          cidrMask: 24,
        },
      ],
    });

    // Create ECS Cluster
    const cluster = new ecs.Cluster(this, 'ScorekeeperCluster', {
      clusterName: `scorekeeper-cluster-${environmentName}`,
      vpc,
      containerInsightsV2: isProd ? ecs.ContainerInsights.ENABLED : ecs.ContainerInsights.DISABLED,
    });

    // Create Fargate Service with Application Load Balancer
    const fargateService = new ecsPatterns.ApplicationLoadBalancedFargateService(
      this,
      'ScorekeeperService',
      {
        serviceName: `scorekeeper-service-${environmentName}`,
        cluster,
        cpu: 256,
        memoryLimitMiB: 512,
        desiredCount,
        publicLoadBalancer: true,
        // Only configure custom domain and HTTPS when certificate is available
        ...(enableHttps && certificate
          ? {
              domainName: apiDomainName,
              domainZone: hostedZone,
              certificate,
              redirectHTTP: true,
              protocol: elbv2.ApplicationProtocol.HTTPS,
            }
          : {}),
        taskImageOptions: {
          image: ecs.ContainerImage.fromEcrRepository(ecrRepository, 'latest'),
          containerName: 'scorekeeper',
          containerPort: 8080,
          environment: {
            ENVIRONMENT: environmentName,
            RUST_LOG: isProd ? 'info' : 'debug',
            S3_AVATAR_BUCKET: avatarBucket.bucketName,
            DYNAMODB_TABLE: table.tableName,
            AWS_REGION: this.region,
          },
        },
        circuitBreaker: {
          rollback: true,
        },
        minHealthyPercent: 100,
        maxHealthyPercent: 200,
      }
    );

    // Configure health check for the target group
    fargateService.targetGroup.configureHealthCheck({
      path: '/health',
      port: '8080',
      healthyHttpCodes: '200',
      healthyThresholdCount: 2,
      unhealthyThresholdCount: 3,
      timeout: cdk.Duration.seconds(5),
      interval: cdk.Duration.seconds(30),
    });

    // Auto-scaling configuration for production
    if (isProd) {
      const scaling = fargateService.service.autoScaleTaskCount({
        minCapacity: 2,
        maxCapacity: 10,
      });

      scaling.scaleOnCpuUtilization('CpuScaling', {
        targetUtilizationPercent: 70,
        scaleInCooldown: cdk.Duration.seconds(60),
        scaleOutCooldown: cdk.Duration.seconds(60),
      });

      scaling.scaleOnMemoryUtilization('MemoryScaling', {
        targetUtilizationPercent: 70,
        scaleInCooldown: cdk.Duration.seconds(60),
        scaleOutCooldown: cdk.Duration.seconds(60),
      });
    }

    // Grant the task execution role permission to pull from ECR
    ecrRepository.grantPull(fargateService.taskDefinition.executionRole!);

    // Grant the task role permission to upload avatars to S3
    avatarBucket.grantPut(fargateService.taskDefinition.taskRole);

    // Grant the task role permission to read/write DynamoDB
    table.grantReadWriteData(fargateService.taskDefinition.taskRole);

    // Outputs
    this.loadBalancerDnsName = new cdk.CfnOutput(this, 'LoadBalancerDnsName', {
      value: fargateService.loadBalancer.loadBalancerDnsName,
      description: 'The DNS name of the Application Load Balancer',
      exportName: `${environmentName}-ScorekeeperAlbDns`,
    });

    this.ecrRepositoryUri = new cdk.CfnOutput(this, 'EcrRepositoryUri', {
      value: ecrRepository.repositoryUri,
      description: 'The URI of the ECR repository',
      exportName: `${environmentName}-ScorekeeperEcrUri`,
    });

    new cdk.CfnOutput(this, 'ClusterName', {
      value: cluster.clusterName,
      description: 'The name of the ECS cluster',
    });

    new cdk.CfnOutput(this, 'ServiceName', {
      value: fargateService.service.serviceName,
      description: 'The name of the ECS service',
    });

    new cdk.CfnOutput(this, 'DynamoDbTableName', {
      value: table.tableName,
      description: 'The name of the DynamoDB table',
    });

    new cdk.CfnOutput(this, 'HostedZoneNameServers', {
      value: cdk.Fn.join(',', hostedZone.hostedZoneNameServers!),
      description: 'Name servers to configure in Namecheap for DNS delegation',
    });

    new cdk.CfnOutput(this, 'ApiDomainName', {
      value: apiDomainName,
      description: 'The API domain name',
    });
  }
}
