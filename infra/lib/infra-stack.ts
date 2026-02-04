import * as cdk from 'aws-cdk-lib/core';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as cr from 'aws-cdk-lib/custom-resources';
import * as dynamodb from 'aws-cdk-lib/aws-dynamodb';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as ecrAssets from 'aws-cdk-lib/aws-ecr-assets';
import * as ecsPatterns from 'aws-cdk-lib/aws-ecs-patterns';
import * as elbv2 from 'aws-cdk-lib/aws-elasticloadbalancingv2';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as route53 from 'aws-cdk-lib/aws-route53';
import * as route53Targets from 'aws-cdk-lib/aws-route53-targets';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import { Construct } from 'constructs';
import * as path from 'path';

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

  /**
   * The name of the S3 bucket for common words (puzzle word selection)
   * @default 'scorekeeper-common-words'
   */
  readonly commonWordsBucketName?: string;

  /**
   * The S3 key for the common words file
   * @default 'common_words.txt'
   */
  readonly commonWordsKey?: string;

  /**
   * ARN of the Secrets Manager secret containing Auth0 M2M credentials.
   * The secret should be a JSON object with 'clientId' and 'clientSecret' fields.
   * If not provided, profile endpoints will not be available.
   */
  readonly auth0M2mSecretArn?: string;

  /**
   * ARN of the Secrets Manager secret containing OTel collector secrets.
   * The secret should be a JSON object with:
   *   - GRAFANA_OTLP_ENDPOINT: Grafana Cloud OTLP gateway URL
   *   - GRAFANA_INSTANCE_ID: Grafana Cloud instance ID (basicauth username)
   *   - GRAFANA_API_KEY: Grafana Cloud API key (basicauth password)
   * If not provided, the collector will use debug exporter only.
   */
  readonly otelSecretsArn?: string;
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
          // Allow public read access for avatar images via bucket policy
          // Block ACLs to prevent individual object ACL changes
          blockPublicAccess: new s3.BlockPublicAccess({
            blockPublicAcls: true,
            ignorePublicAcls: true,
            blockPublicPolicy: false,
            restrictPublicBuckets: false,
          }),
          encryption: s3.BucketEncryption.S3_MANAGED,
          enforceSSL: true,
          removalPolicy: cdk.RemovalPolicy.RETAIN,
          cors: [
            {
              allowedMethods: [s3.HttpMethods.PUT, s3.HttpMethods.GET],
              allowedOrigins: ['*'],
              allowedHeaders: ['*'],
              maxAge: 3600,
            },
          ],
        });

    // For imported buckets, we need to update the public access block settings
    // before we can add a public bucket policy
    if (props?.importExistingAvatarBucket) {
      const updatePublicAccessBlock = new cr.AwsCustomResource(
        this,
        'UpdateAvatarBucketPublicAccessBlock',
        {
          onCreate: {
            service: 'S3',
            action: 'putPublicAccessBlock',
            parameters: {
              Bucket: avatarBucketName,
              PublicAccessBlockConfiguration: {
                BlockPublicAcls: true,
                IgnorePublicAcls: true,
                BlockPublicPolicy: false,
                RestrictPublicBuckets: false,
              },
            },
            physicalResourceId: cr.PhysicalResourceId.of(
              `${avatarBucketName}-public-access-block`
            ),
          },
          onUpdate: {
            service: 'S3',
            action: 'putPublicAccessBlock',
            parameters: {
              Bucket: avatarBucketName,
              PublicAccessBlockConfiguration: {
                BlockPublicAcls: true,
                IgnorePublicAcls: true,
                BlockPublicPolicy: false,
                RestrictPublicBuckets: false,
              },
            },
            physicalResourceId: cr.PhysicalResourceId.of(
              `${avatarBucketName}-public-access-block`
            ),
          },
          policy: cr.AwsCustomResourcePolicy.fromStatements([
            new iam.PolicyStatement({
              actions: [
                's3:PutBucketPublicAccessBlock',
                's3:GetBucketPublicAccessBlock',
              ],
              resources: [`arn:aws:s3:::${avatarBucketName}`],
            }),
          ]),
        }
      );

      // Create the bucket policy and ensure it depends on the public access block update
      const bucketPolicy = new s3.BucketPolicy(this, 'AvatarBucketPolicy', {
        bucket: avatarBucket,
      });
      bucketPolicy.document.addStatements(
        new iam.PolicyStatement({
          sid: 'PublicReadAvatars',
          effect: iam.Effect.ALLOW,
          principals: [new iam.AnyPrincipal()],
          actions: ['s3:GetObject'],
          resources: [`${avatarBucket.bucketArn}/avatars/*`],
        })
      );
      bucketPolicy.node.addDependency(updatePublicAccessBlock);
    } else {
      // For new buckets, just add the policy directly (public access is already configured)
      new s3.BucketPolicy(this, 'AvatarBucketPolicy', {
        bucket: avatarBucket,
      }).document.addStatements(
        new iam.PolicyStatement({
          sid: 'PublicReadAvatars',
          effect: iam.Effect.ALLOW,
          principals: [new iam.AnyPrincipal()],
          actions: ['s3:GetObject'],
          resources: [`${avatarBucket.bucketArn}/avatars/*`],
        })
      );
    }

    // S3 bucket for common words (private - used for puzzle word selection)
    const commonWordsBucketName = props?.commonWordsBucketName ?? 'scorekeeper-common-words';
    const commonWordsKey = props?.commonWordsKey ?? 'common_words.txt';

    const commonWordsBucket = new s3.Bucket(this, 'CommonWordsBucket', {
      bucketName: commonWordsBucketName,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    // Route 53 Hosted Zone for the API subdomain (NS records configured in Namecheap)
    const domainName = this.node.tryGetContext('domainName') ?? 'wordles.dev';
    const apiSubdomain = this.node.tryGetContext('apiSubdomain') ?? 'api';
    const apiDomainName = `${apiSubdomain}.${domainName}`;

    // HTTPS enabled by default. For initial deployment before DNS is configured,
    // use -c enableHttps=false to deploy without certificate, then redeploy once
    // NS records are configured in Namecheap.
    const enableHttps = this.node.tryGetContext('enableHttps') !== 'false';

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

    // Look up Auth0 M2M secret if ARN is provided (required for profile endpoints)
    const auth0M2mSecretArn = props?.auth0M2mSecretArn ?? this.node.tryGetContext('auth0M2mSecretArn');
    const auth0M2mSecret = auth0M2mSecretArn
      ? secretsmanager.Secret.fromSecretCompleteArn(this, 'Auth0M2mSecret', auth0M2mSecretArn)
      : undefined;

    // Look up OTel secrets if ARN is provided (for Grafana Cloud credentials)
    const otelSecretsArn = props?.otelSecretsArn ?? this.node.tryGetContext('otelSecretsArn');
    const otelSecrets = otelSecretsArn
      ? secretsmanager.Secret.fromSecretCompleteArn(this, 'OtelSecrets', otelSecretsArn)
      : undefined;

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
            S3_COMMON_WORDS_BUCKET: commonWordsBucket.bucketName,
            S3_COMMON_WORDS_KEY: commonWordsKey,
            DYNAMODB_TABLE: table.tableName,
            AWS_REGION: this.region,
            // OTel configuration - collector sidecar on localhost
            OTEL_EXPORTER_OTLP_ENDPOINT: 'http://localhost:4317',
            APP_VERSION: 'latest',
          },
          // Auth0 M2M credentials for profile endpoints (from Secrets Manager)
          ...(auth0M2mSecret
            ? {
                secrets: {
                  AUTH0_M2M_CLIENT_ID: ecs.Secret.fromSecretsManager(auth0M2mSecret, 'clientId'),
                  AUTH0_M2M_CLIENT_SECRET: ecs.Secret.fromSecretsManager(auth0M2mSecret, 'clientSecret'),
                },
              }
            : {}),
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

    // Build OTel collector image from local Dockerfile
    const collectorImage = new ecrAssets.DockerImageAsset(this, 'CollectorImage', {
      directory: path.join(__dirname, '../../collector'),
    });

    // Add OTel collector sidecar container to the task definition
    const collectorContainer = fargateService.taskDefinition.addContainer('otel-collector', {
      image: ecs.ContainerImage.fromDockerImageAsset(collectorImage),
      memoryLimitMiB: 128,
      cpu: 64,
      essential: false, // Don't kill task if collector crashes
      logging: ecs.LogDrivers.awsLogs({
        streamPrefix: 'otel-collector',
        logRetention: logs.RetentionDays.ONE_WEEK,
      }),
      environment: {
        ENVIRONMENT: environmentName,
      },
      // Grafana Cloud credentials from Secrets Manager (if configured)
      ...(otelSecrets
        ? {
            secrets: {
              GRAFANA_OTLP_ENDPOINT: ecs.Secret.fromSecretsManager(otelSecrets, 'GRAFANA_OTLP_ENDPOINT'),
              GRAFANA_INSTANCE_ID: ecs.Secret.fromSecretsManager(otelSecrets, 'GRAFANA_INSTANCE_ID'),
              GRAFANA_API_KEY: ecs.Secret.fromSecretsManager(otelSecrets, 'GRAFANA_API_KEY'),
            },
          }
        : {}),
    });

    // Add port mappings for the collector
    collectorContainer.addPortMappings(
      { containerPort: 4317, protocol: ecs.Protocol.TCP }, // OTLP gRPC (backend)
      { containerPort: 4318, protocol: ecs.Protocol.TCP }, // OTLP HTTP (frontend)
      { containerPort: 13133, protocol: ecs.Protocol.TCP } // health check
    );

    // Add target group for OTLP HTTP (frontend traces) - /v1/traces path
    const otelTargetGroup = new elbv2.ApplicationTargetGroup(this, 'OtelHttpTarget', {
      vpc,
      port: 4318,
      protocol: elbv2.ApplicationProtocol.HTTP,
      targetGroupName: `otel-collector-${environmentName}`,
      targetType: elbv2.TargetType.IP,
      healthCheck: {
        path: '/',
        port: '13133',
        healthyHttpCodes: '200',
      },
    });

    // Register the service with the OTel target group
    fargateService.service.registerLoadBalancerTargets({
      containerName: 'otel-collector',
      containerPort: 4318,
      newTargetGroupId: 'otel-collector',
      listener: ecs.ListenerConfig.applicationListener(fargateService.listener, {
        protocol: elbv2.ApplicationProtocol.HTTPS,
        conditions: [elbv2.ListenerCondition.pathPatterns(['/v1/traces'])],
        priority: 10,
      }),
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

    // Grant the task role permission to read common words from S3
    commonWordsBucket.grantRead(fargateService.taskDefinition.taskRole);

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

    new cdk.CfnOutput(this, 'CommonWordsBucketName', {
      value: commonWordsBucket.bucketName,
      description: 'The S3 bucket for common words (puzzle selection)',
    });
  }
}
