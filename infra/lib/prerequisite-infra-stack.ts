import * as cdk from 'aws-cdk-lib/core';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as s3 from 'aws-cdk-lib/aws-s3';
import { Construct } from 'constructs';

export interface PrerequisiteInfraStackProps extends cdk.StackProps {
  /**
   * The name of the ECR repository
   * @default 'scorekeeper'
   */
  readonly repositoryName?: string;

  /**
   * The name of the S3 bucket for avatar uploads
   * @default 'scorekeeper-avatars'
   */
  readonly avatarBucketName?: string;
}

export class PrerequisiteInfraStack extends cdk.Stack {
  public readonly repositoryUri: cdk.CfnOutput;
  public readonly repositoryArn: cdk.CfnOutput;
  public readonly avatarBucketName: cdk.CfnOutput;
  public readonly avatarBucketArn: cdk.CfnOutput;

  constructor(scope: Construct, id: string, props?: PrerequisiteInfraStackProps) {
    super(scope, id, props);

    const repositoryName = props?.repositoryName ?? 'scorekeeper';
    const avatarBucketName = props?.avatarBucketName ?? 'scorekeeper-avatars';

    // Create ECR Repository for the Docker image
    const ecrRepository = new ecr.Repository(this, 'ScorekeeperEcrRepository', {
      repositoryName,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      imageScanOnPush: true,
      encryption: ecr.RepositoryEncryption.AES_256,
      lifecycleRules: [
        {
          maxImageCount: 25,
          description: 'Keep only recent images',
        },
      ],
    });

    // Create S3 bucket for avatar uploads
    const avatarBucket = new s3.Bucket(this, 'AvatarBucket', {
      bucketName: avatarBucketName,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      enforceSSL: true,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      cors: [
        {
          allowedMethods: [s3.HttpMethods.PUT],
          allowedOrigins: ['*'], // Will be restricted by IAM and pre-signed URLs
          allowedHeaders: ['*'],
          maxAge: 3600,
        },
      ],
    });

    // Outputs
    this.repositoryUri = new cdk.CfnOutput(this, 'EcrRepositoryUri', {
      value: ecrRepository.repositoryUri,
      description: 'The URI of the ECR repository',
      exportName: 'ScorekeeperEcrRepositoryUri',
    });

    this.repositoryArn = new cdk.CfnOutput(this, 'EcrRepositoryArn', {
      value: ecrRepository.repositoryArn,
      description: 'The ARN of the ECR repository',
      exportName: 'ScorekeeperEcrRepositoryArn',
    });

    this.avatarBucketName = new cdk.CfnOutput(this, 'AvatarBucketName', {
      value: avatarBucket.bucketName,
      description: 'The name of the S3 bucket for avatar uploads',
      exportName: 'ScorekeeperAvatarBucketName',
    });

    this.avatarBucketArn = new cdk.CfnOutput(this, 'AvatarBucketArn', {
      value: avatarBucket.bucketArn,
      description: 'The ARN of the S3 bucket for avatar uploads',
      exportName: 'ScorekeeperAvatarBucketArn',
    });
  }
}
