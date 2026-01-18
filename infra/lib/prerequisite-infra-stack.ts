import * as cdk from 'aws-cdk-lib/core';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import { Construct } from 'constructs';

export interface PrerequisiteInfraStackProps extends cdk.StackProps {
  /**
   * The name of the ECR repository
   * @default 'scorekeeper'
   */
  readonly repositoryName?: string;
}

export class PrerequisiteInfraStack extends cdk.Stack {
  public readonly repositoryUri: cdk.CfnOutput;
  public readonly repositoryArn: cdk.CfnOutput;

  constructor(scope: Construct, id: string, props?: PrerequisiteInfraStackProps) {
    super(scope, id, props);

    const repositoryName = props?.repositoryName ?? 'scorekeeper';

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
  }
}
