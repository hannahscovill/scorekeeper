import * as cdk from 'aws-cdk-lib/core';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as dynamodb from 'aws-cdk-lib/aws-dynamodb';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as ecrAssets from 'aws-cdk-lib/aws-ecr-assets';
import * as ecsPatterns from 'aws-cdk-lib/aws-ecs-patterns';
import * as elbv2 from 'aws-cdk-lib/aws-elasticloadbalancingv2';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as route53 from 'aws-cdk-lib/aws-route53';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import * as ssm from 'aws-cdk-lib/aws-ssm';
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
   * ARN of the Secrets Manager secret containing GitHub App credentials for the issue proxy.
   * The secret should be a JSON object with 'appId', 'installationId', and 'privateKey' fields.
   * If not provided, the /issues endpoint will not be available.
   */
  readonly githubAppSecretArn?: string;

  /**
   * SSM parameter name for the Turnstile secret key (used for issue proxy CAPTCHA).
   * @default '/wordles/turnstile-secret-key'
   */
  readonly turnstileSsmParamName?: string;

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

    const environmentName = props?.environmentName ?? 'prod';
    const commitHash = this.node.tryGetContext('commitHash') ?? 'dev-0000000';
    const desiredCount = props?.desiredCount ?? 2;

    // Look up the existing ECR Repository created by PrerequisiteInfraStack
    const ecrRepository = ecr.Repository.fromRepositoryName(
      this,
      'ScorekeeperRepository',
      'scorekeeper'
    );

    // S3 bucket for avatar uploads (private — accessed via pre-signed URLs)
    const avatarBucketName = props?.avatarBucketName ?? 'scorekeeper-avatars';
    const avatarBucket = new s3.Bucket(this, 'AvatarBucket', {
      bucketName: avatarBucketName,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    // S3 bucket for common words (private - used for puzzle word selection)
    const commonWordsBucketName = props?.commonWordsBucketName ?? 'scorekeeper-common-words';
    const commonWordsKey = props?.commonWordsKey ?? 'common_words.txt';

    const commonWordsBucket = new s3.Bucket(this, 'CommonWordsBucket', {
      bucketName: commonWordsBucketName,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    // Look up the hosted zone created by PrerequisiteInfraStack
    const apiDomainName = 'api.wordles.dev';
    const hostedZone = route53.HostedZone.fromLookup(this, 'HostedZone', {
      domainName: apiDomainName,
    });

    // ACM Certificate for the API subdomain with DNS validation
    const certificate = new acm.Certificate(this, 'ApiCertificate', {
      domainName: apiDomainName,
      validation: acm.CertificateValidation.fromDns(hostedZone),
    });

    // DynamoDB table for game data (single-table design)
    const table = new dynamodb.Table(this, 'ScorekeeperTable', {
      tableName: `scorekeeper-${environmentName}`,
      partitionKey: { name: 'pk', type: dynamodb.AttributeType.STRING },
      sortKey: { name: 'sk', type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
    });

    table.addGlobalSecondaryIndex({
      indexName: 'GameSessionIndex',
      partitionKey: { name: 'game_id', type: dynamodb.AttributeType.STRING },
      projectionType: dynamodb.ProjectionType.ALL,
    });

    // Create VPC - using a simple 2-AZ VPC setup
    const vpc = new ec2.Vpc(this, 'ScorekeeperVpc', {
      vpcName: `scorekeeper-vpc-${environmentName}`,
      maxAzs: 2,
      natGateways: 2,
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
      containerInsightsV2: ecs.ContainerInsights.ENABLED,
    });

    // Look up Auth0 M2M secret if ARN is provided (required for profile endpoints)
    const auth0M2mSecretArn = props?.auth0M2mSecretArn ?? this.node.tryGetContext('auth0M2mSecretArn');
    const auth0M2mSecret = auth0M2mSecretArn
      ? secretsmanager.Secret.fromSecretCompleteArn(this, 'Auth0M2mSecret', auth0M2mSecretArn)
      : undefined;

    // Look up GitHub App secret if ARN is provided (for issue proxy)
    const githubAppSecretArn = props?.githubAppSecretArn ?? this.node.tryGetContext('githubAppSecretArn');
    const githubAppSecret = githubAppSecretArn
      ? secretsmanager.Secret.fromSecretCompleteArn(this, 'GitHubAppSecret', githubAppSecretArn)
      : undefined;

    // Look up Turnstile SSM parameter (for issue proxy CAPTCHA verification)
    const turnstileSsmParamName = props?.turnstileSsmParamName
      ?? this.node.tryGetContext('turnstileSsmParamName')
      ?? '/wordles/turnstile-secret-key';
    const turnstileParam = githubAppSecret
      ? ssm.StringParameter.fromSecureStringParameterAttributes(this, 'TurnstileSecretKey', {
          parameterName: turnstileSsmParamName,
        })
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
        domainName: apiDomainName,
        domainZone: hostedZone,
        certificate,
        redirectHTTP: true,
        protocol: elbv2.ApplicationProtocol.HTTPS,
        taskImageOptions: {
          image: ecs.ContainerImage.fromEcrRepository(ecrRepository, 'latest'),
          containerName: 'scorekeeper',
          containerPort: 8080,
          environment: {
            ENVIRONMENT: 'production',
            RUST_LOG: 'info',
            S3_AVATAR_BUCKET: avatarBucket.bucketName,
            S3_COMMON_WORDS_BUCKET: commonWordsBucket.bucketName,
            S3_COMMON_WORDS_KEY: commonWordsKey,
            DYNAMODB_TABLE_NAME: table.tableName,
            AWS_REGION: this.region,
            // OTel configuration - collector sidecar on localhost
            OTEL_EXPORTER_OTLP_ENDPOINT: 'http://localhost:4317',
            COMMIT_HASH: commitHash,
          },
          // Secrets injected into the container at runtime
          secrets: {
            // Auth0 M2M credentials for profile endpoints
            ...(auth0M2mSecret && {
              AUTH0_M2M_CLIENT_ID: ecs.Secret.fromSecretsManager(auth0M2mSecret, 'clientId'),
              AUTH0_M2M_CLIENT_SECRET: ecs.Secret.fromSecretsManager(auth0M2mSecret, 'clientSecret'),
            }),
            // GitHub App credentials for issue proxy
            ...(githubAppSecret && {
              GITHUB_APP_ID: ecs.Secret.fromSecretsManager(githubAppSecret, 'appId'),
              GITHUB_INSTALLATION_ID: ecs.Secret.fromSecretsManager(githubAppSecret, 'installationId'),
              GITHUB_PRIVATE_KEY: ecs.Secret.fromSecretsManager(githubAppSecret, 'privateKey'),
            }),
            // Turnstile CAPTCHA secret for issue proxy
            ...(turnstileParam && {
              TURNSTILE_SECRET_KEY: ecs.Secret.fromSsmParameter(turnstileParam),
            }),
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

    // OTel collector sidecar — only deploy when Grafana secrets are configured.
    // Without secrets the collector crashes, its ALB target group fails health
    // checks, and ECS circuit breaker rolls back the entire deployment.
    // The scorekeeper app's OTel SDK handles connection-refused gracefully (drops spans).
    if (otelSecrets) {
      const collectorImage = new ecrAssets.DockerImageAsset(this, 'CollectorImage', {
        directory: path.join(__dirname, '../../collector'),
      });

      const collectorContainer = fargateService.taskDefinition.addContainer('otel-collector', {
        image: ecs.ContainerImage.fromDockerImageAsset(collectorImage),
        memoryLimitMiB: 128,
        cpu: 64,
        essential: false,
        logging: ecs.LogDrivers.awsLogs({
          streamPrefix: 'otel-collector',
          logRetention: logs.RetentionDays.ONE_WEEK,
        }),
        environment: {
          ENVIRONMENT: 'production',
        },
        secrets: {
          GRAFANA_OTLP_ENDPOINT: ecs.Secret.fromSecretsManager(otelSecrets, 'GRAFANA_OTLP_ENDPOINT'),
          GRAFANA_INSTANCE_ID: ecs.Secret.fromSecretsManager(otelSecrets, 'GRAFANA_INSTANCE_ID'),
          GRAFANA_API_KEY: ecs.Secret.fromSecretsManager(otelSecrets, 'GRAFANA_API_KEY'),
        },
      });

      collectorContainer.addPortMappings(
        { containerPort: 4317, protocol: ecs.Protocol.TCP },
        { containerPort: 4318, protocol: ecs.Protocol.TCP },
        { containerPort: 13133, protocol: ecs.Protocol.TCP },
      );

      fargateService.service.registerLoadBalancerTargets({
        containerName: 'otel-collector',
        containerPort: 4318,
        newTargetGroupId: 'otel-collector',
        listener: ecs.ListenerConfig.applicationListener(fargateService.listener, {
          protocol: elbv2.ApplicationProtocol.HTTP,
          conditions: [elbv2.ListenerCondition.pathPatterns(['/v1/traces'])],
          priority: 10,
          healthCheck: {
            path: '/',
            port: '13133',
            healthyHttpCodes: '200',
          },
        }),
      });

      // Allow ALB to reach the collector health check port (13133).
      // CDK only auto-creates SG rules for the traffic port (4318).
      fargateService.service.connections.allowFrom(
        fargateService.loadBalancer,
        ec2.Port.tcp(13133),
        'ALB to OTel collector health check',
      );
    }

    // Auto-scaling
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

    // Grant the task execution role permission to pull from ECR
    ecrRepository.grantPull(fargateService.taskDefinition.executionRole!);

    // Grant the task role permission to read and upload avatars to S3
    avatarBucket.grantRead(fargateService.taskDefinition.taskRole);
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

    new cdk.CfnOutput(this, 'CommonWordsBucketName', {
      value: commonWordsBucket.bucketName,
      description: 'The S3 bucket for common words (puzzle selection)',
    });
  }
}
