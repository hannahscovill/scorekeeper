import * as cdk from 'aws-cdk-lib/core';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as route53 from 'aws-cdk-lib/aws-route53';
import { Construct } from 'constructs';

export interface PrerequisiteInfraStackProps extends cdk.StackProps {
  /**
   * The name of the ECR repository
   * @default 'scorekeeper'
   */
  readonly repositoryName?: string;

  /**
   * The domain name for the API hosted zone
   * @default 'api.wordles.dev'
   */
  readonly apiDomainName?: string;
}

export class PrerequisiteInfraStack extends cdk.Stack {
  public readonly repositoryUri: cdk.CfnOutput;
  public readonly repositoryArn: cdk.CfnOutput;

  constructor(scope: Construct, id: string, props?: PrerequisiteInfraStackProps) {
    super(scope, id, props);

    const repositoryName = props?.repositoryName ?? 'scorekeeper';
    const apiDomainName = props?.apiDomainName ?? 'api.wordles.dev';

    // ECR Repository for the Docker image
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

    // Route 53 Hosted Zone for the API subdomain
    // Lives here so it survives app stack deletions. NS records must be
    // configured at the registrar (Namecheap) after first deploy.
    const hostedZone = new route53.HostedZone(this, 'ApiHostedZone', {
      zoneName: apiDomainName,
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

    new cdk.CfnOutput(this, 'HostedZoneId', {
      value: hostedZone.hostedZoneId,
      description: 'The Route53 hosted zone ID for api.wordles.dev',
      exportName: 'ScorekeeperApiHostedZoneId',
    });

    new cdk.CfnOutput(this, 'HostedZoneNameServers', {
      value: cdk.Fn.join(',', hostedZone.hostedZoneNameServers!),
      description: 'NS records to configure at the registrar',
    });
  }
}
