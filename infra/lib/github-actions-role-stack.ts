import * as cdk from 'aws-cdk-lib/core';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Construct } from 'constructs';

export interface GitHubActionsRoleStackProps extends cdk.StackProps {
  /**
   * The GitHub organization or username
   */
  readonly githubOrg: string;

  /**
   * The GitHub repository name
   */
  readonly githubRepo: string;

  /**
   * Branches allowed to assume this role
   * @default ['main']
   */
  readonly allowedBranches?: string[];
}

/**
 * Creates an IAM role for GitHub Actions to deploy the scorekeeper service.
 *
 * This role references the shared OIDC provider created by GitHubOidcProviderStack.
 * Permissions are scoped to only what scorekeeper needs.
 */
export class GitHubActionsRoleStack extends cdk.Stack {
  public readonly roleArn: string;

  constructor(scope: Construct, id: string, props: GitHubActionsRoleStackProps) {
    super(scope, id, props);

    const { githubOrg, githubRepo, allowedBranches = ['main'] } = props;

    // Import the shared OIDC provider ARN
    const oidcProviderArn = cdk.Fn.importValue('GitHubOidcProviderArn');

    // Build subject conditions for allowed branches
    const subjectConditions = allowedBranches
      .map((branch) => `repo:${githubOrg}/${githubRepo}:ref:refs/heads/${branch}`)
      .concat([`repo:${githubOrg}/${githubRepo}:*`]);

    // Create the IAM role
    const role = new iam.Role(this, 'GitHubActionsRole', {
      roleName: `GitHubActions-${githubRepo}`,
      description: `Role for GitHub Actions to deploy ${githubRepo}`,
      maxSessionDuration: cdk.Duration.hours(1),
      assumedBy: new iam.FederatedPrincipal(
        oidcProviderArn,
        {
          StringEquals: {
            'token.actions.githubusercontent.com:aud': 'sts.amazonaws.com',
          },
          StringLike: {
            'token.actions.githubusercontent.com:sub': subjectConditions,
          },
        },
        'sts:AssumeRoleWithWebIdentity'
      ),
    });

    // ECR permissions - auth
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECRAuth',
        effect: iam.Effect.ALLOW,
        actions: ['ecr:GetAuthorizationToken'],
        resources: ['*'],
      })
    );

    // ECR permissions - push and pull images
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECRPushPull',
        effect: iam.Effect.ALLOW,
        actions: [
          'ecr:BatchCheckLayerAvailability',
          'ecr:BatchGetImage',
          'ecr:CompleteLayerUpload',
          'ecr:GetDownloadUrlForLayer',
          'ecr:InitiateLayerUpload',
          'ecr:PutImage',
          'ecr:UploadLayerPart',
        ],
        resources: [
          `arn:aws:ecr:*:${this.account}:repository/scorekeeper`,
          `arn:aws:ecr:*:${this.account}:repository/scorekeeper-*`,
        ],
      })
    );

    // ECR permissions - manage repository
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECRManage',
        effect: iam.Effect.ALLOW,
        actions: [
          'ecr:CreateRepository',
          'ecr:DeleteRepository',
          'ecr:DescribeRepositories',
          'ecr:PutImageScanningConfiguration',
          'ecr:PutLifecyclePolicy',
          'ecr:GetLifecyclePolicy',
          'ecr:DeleteLifecyclePolicy',
          'ecr:TagResource',
          'ecr:UntagResource',
          'ecr:ListTagsForResource',
          'ecr:SetRepositoryPolicy',
          'ecr:GetRepositoryPolicy',
          'ecr:DeleteRepositoryPolicy',
        ],
        resources: [
          `arn:aws:ecr:*:${this.account}:repository/scorekeeper`,
          `arn:aws:ecr:*:${this.account}:repository/scorekeeper-*`,
        ],
      })
    );

    // CloudFormation permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'CloudFormation',
        effect: iam.Effect.ALLOW,
        actions: [
          'cloudformation:CreateStack',
          'cloudformation:UpdateStack',
          'cloudformation:DeleteStack',
          'cloudformation:DescribeStacks',
          'cloudformation:DescribeStackEvents',
          'cloudformation:DescribeStackResources',
          'cloudformation:GetTemplate',
          'cloudformation:ValidateTemplate',
          'cloudformation:CreateChangeSet',
          'cloudformation:DescribeChangeSet',
          'cloudformation:ExecuteChangeSet',
          'cloudformation:DeleteChangeSet',
          'cloudformation:GetTemplateSummary',
        ],
        resources: [
          `arn:aws:cloudformation:*:${this.account}:stack/ScorekeeperStack-*/*`,
          `arn:aws:cloudformation:*:${this.account}:stack/PrerequisiteInfraStack/*`,
          `arn:aws:cloudformation:*:${this.account}:stack/CDKToolkit/*`,
        ],
      })
    );

    // CloudFormation read permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'CloudFormationRead',
        effect: iam.Effect.ALLOW,
        actions: [
          'cloudformation:DescribeStacks',
          'cloudformation:DescribeStackEvents',
          'cloudformation:ListStacks',
        ],
        resources: ['*'],
      })
    );

    // ECS permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECS',
        effect: iam.Effect.ALLOW,
        actions: [
          'ecs:CreateCluster',
          'ecs:DeleteCluster',
          'ecs:DescribeClusters',
          'ecs:CreateService',
          'ecs:UpdateService',
          'ecs:DeleteService',
          'ecs:DescribeServices',
          'ecs:RegisterTaskDefinition',
          'ecs:DeregisterTaskDefinition',
          'ecs:DescribeTaskDefinition',
          'ecs:ListTaskDefinitions',
          'ecs:TagResource',
          'ecs:UntagResource',
        ],
        resources: ['*'],
        conditions: {
          StringEquals: {
            'aws:ResourceAccount': this.account,
          },
        },
      })
    );

    // ECS wait/describe permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECSWait',
        effect: iam.Effect.ALLOW,
        actions: ['ecs:DescribeTasks', 'ecs:ListTasks'],
        resources: ['*'],
      })
    );

    // VPC/EC2 permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'VPC',
        effect: iam.Effect.ALLOW,
        actions: [
          'ec2:CreateVpc',
          'ec2:DeleteVpc',
          'ec2:DescribeVpcs',
          'ec2:ModifyVpcAttribute',
          'ec2:CreateSubnet',
          'ec2:DeleteSubnet',
          'ec2:DescribeSubnets',
          'ec2:CreateRouteTable',
          'ec2:DeleteRouteTable',
          'ec2:DescribeRouteTables',
          'ec2:AssociateRouteTable',
          'ec2:DisassociateRouteTable',
          'ec2:CreateRoute',
          'ec2:DeleteRoute',
          'ec2:CreateInternetGateway',
          'ec2:DeleteInternetGateway',
          'ec2:AttachInternetGateway',
          'ec2:DetachInternetGateway',
          'ec2:DescribeInternetGateways',
          'ec2:AllocateAddress',
          'ec2:ReleaseAddress',
          'ec2:DescribeAddresses',
          'ec2:CreateNatGateway',
          'ec2:DeleteNatGateway',
          'ec2:DescribeNatGateways',
          'ec2:CreateSecurityGroup',
          'ec2:DeleteSecurityGroup',
          'ec2:DescribeSecurityGroups',
          'ec2:AuthorizeSecurityGroupIngress',
          'ec2:AuthorizeSecurityGroupEgress',
          'ec2:RevokeSecurityGroupIngress',
          'ec2:RevokeSecurityGroupEgress',
          'ec2:CreateTags',
          'ec2:DeleteTags',
          'ec2:DescribeTags',
          'ec2:DescribeAvailabilityZones',
          'ec2:DescribeAccountAttributes',
        ],
        resources: ['*'],
      })
    );

    // ELB permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ELB',
        effect: iam.Effect.ALLOW,
        actions: [
          'elasticloadbalancing:CreateLoadBalancer',
          'elasticloadbalancing:DeleteLoadBalancer',
          'elasticloadbalancing:DescribeLoadBalancers',
          'elasticloadbalancing:ModifyLoadBalancerAttributes',
          'elasticloadbalancing:DescribeLoadBalancerAttributes',
          'elasticloadbalancing:CreateTargetGroup',
          'elasticloadbalancing:DeleteTargetGroup',
          'elasticloadbalancing:DescribeTargetGroups',
          'elasticloadbalancing:ModifyTargetGroupAttributes',
          'elasticloadbalancing:DescribeTargetGroupAttributes',
          'elasticloadbalancing:CreateListener',
          'elasticloadbalancing:DeleteListener',
          'elasticloadbalancing:DescribeListeners',
          'elasticloadbalancing:ModifyListener',
          'elasticloadbalancing:RegisterTargets',
          'elasticloadbalancing:DeregisterTargets',
          'elasticloadbalancing:DescribeTargetHealth',
          'elasticloadbalancing:AddTags',
          'elasticloadbalancing:RemoveTags',
          'elasticloadbalancing:DescribeTags',
        ],
        resources: ['*'],
      })
    );

    // IAM permissions (scoped to scorekeeper roles)
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'IAM',
        effect: iam.Effect.ALLOW,
        actions: [
          'iam:CreateRole',
          'iam:DeleteRole',
          'iam:GetRole',
          'iam:UpdateRole',
          'iam:PassRole',
          'iam:AttachRolePolicy',
          'iam:DetachRolePolicy',
          'iam:PutRolePolicy',
          'iam:DeleteRolePolicy',
          'iam:GetRolePolicy',
          'iam:ListRolePolicies',
          'iam:ListAttachedRolePolicies',
          'iam:TagRole',
          'iam:UntagRole',
          'iam:ListRoleTags',
        ],
        resources: [
          `arn:aws:iam::${this.account}:role/ScorekeeperStack-*`,
          `arn:aws:iam::${this.account}:role/cdk-*`,
        ],
      })
    );

    // STS permissions - assume CDK bootstrap roles
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'AssumeBootstrapRoles',
        effect: iam.Effect.ALLOW,
        actions: ['sts:AssumeRole'],
        resources: [
          `arn:aws:iam::${this.account}:role/cdk-*-deploy-role-${this.account}-*`,
          `arn:aws:iam::${this.account}:role/cdk-*-file-publishing-role-${this.account}-*`,
          `arn:aws:iam::${this.account}:role/cdk-*-image-publishing-role-${this.account}-*`,
          `arn:aws:iam::${this.account}:role/cdk-*-lookup-role-${this.account}-*`,
        ],
      })
    );

    // S3 permissions - CDK assets
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'S3CDKAssets',
        effect: iam.Effect.ALLOW,
        actions: [
          's3:GetObject',
          's3:PutObject',
          's3:DeleteObject',
          's3:ListBucket',
          's3:GetBucketLocation',
        ],
        resources: [
          `arn:aws:s3:::cdk-*-assets-${this.account}-*`,
          `arn:aws:s3:::cdk-*-assets-${this.account}-*/*`,
        ],
      })
    );

    // S3 permissions - scorekeeper avatars bucket
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'S3AppBuckets',
        effect: iam.Effect.ALLOW,
        actions: [
          's3:CreateBucket',
          's3:DeleteBucket',
          's3:GetBucketPolicy',
          's3:PutBucketPolicy',
          's3:DeleteBucketPolicy',
          's3:GetBucketAcl',
          's3:PutBucketAcl',
          's3:GetBucketCORS',
          's3:PutBucketCORS',
          's3:DeleteBucketCORS',
          's3:GetBucketPublicAccessBlock',
          's3:PutBucketPublicAccessBlock',
          's3:GetEncryptionConfiguration',
          's3:PutEncryptionConfiguration',
          's3:GetBucketTagging',
          's3:PutBucketTagging',
          's3:GetBucketVersioning',
          's3:PutBucketVersioning',
          's3:GetLifecycleConfiguration',
          's3:PutLifecycleConfiguration',
          's3:ListBucket',
          's3:GetObject',
          's3:PutObject',
          's3:DeleteObject',
        ],
        resources: [
          'arn:aws:s3:::scorekeeper-avatars',
          'arn:aws:s3:::scorekeeper-avatars/*',
        ],
      })
    );

    // DynamoDB permissions
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'DynamoDB',
        effect: iam.Effect.ALLOW,
        actions: [
          'dynamodb:CreateTable',
          'dynamodb:DeleteTable',
          'dynamodb:DescribeTable',
          'dynamodb:UpdateTable',
          'dynamodb:DescribeTimeToLive',
          'dynamodb:UpdateTimeToLive',
          'dynamodb:DescribeContinuousBackups',
          'dynamodb:UpdateContinuousBackups',
          'dynamodb:ListTagsOfResource',
          'dynamodb:TagResource',
          'dynamodb:UntagResource',
        ],
        resources: [`arn:aws:dynamodb:*:${this.account}:table/scorekeeper-*`],
      })
    );

    // Terraform state - S3 bucket for state storage
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'TerraformStateS3',
        effect: iam.Effect.ALLOW,
        actions: [
          's3:GetObject',
          's3:PutObject',
          's3:DeleteObject',
          's3:ListBucket',
          's3:GetBucketVersioning',
        ],
        resources: [
          `arn:aws:s3:::orchestra-tfstate-${this.account}`,
          `arn:aws:s3:::orchestra-tfstate-${this.account}/*`,
        ],
      })
    );

    // Terraform state - DynamoDB table for state locking
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'TerraformStateLock',
        effect: iam.Effect.ALLOW,
        actions: [
          'dynamodb:GetItem',
          'dynamodb:PutItem',
          'dynamodb:DeleteItem',
        ],
        resources: [
          `arn:aws:dynamodb:*:${this.account}:table/orchestra-terraform-locks`,
        ],
      })
    );

    // SSM Parameter Store - CDK bootstrap
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'SSM',
        effect: iam.Effect.ALLOW,
        actions: ['ssm:GetParameter', 'ssm:GetParameters'],
        resources: [`arn:aws:ssm:*:${this.account}:parameter/cdk-bootstrap/*`],
      })
    );

    // Secrets Manager - Auth0 M2M credentials
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'SecretsManager',
        effect: iam.Effect.ALLOW,
        actions: ['secretsmanager:GetSecretValue', 'secretsmanager:DescribeSecret'],
        resources: [`arn:aws:secretsmanager:*:${this.account}:secret:scorekeeper/*`],
      })
    );

    // CloudWatch Logs
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'Logs',
        effect: iam.Effect.ALLOW,
        actions: [
          'logs:CreateLogGroup',
          'logs:DeleteLogGroup',
          'logs:DescribeLogGroups',
          'logs:PutRetentionPolicy',
          'logs:DeleteRetentionPolicy',
          'logs:TagLogGroup',
          'logs:UntagLogGroup',
          'logs:ListTagsLogGroup',
        ],
        resources: [
          `arn:aws:logs:*:${this.account}:log-group:/ecs/scorekeeper-*`,
          `arn:aws:logs:*:${this.account}:log-group:/ecs/scorekeeper-*:*`,
        ],
      })
    );

    // Application Auto Scaling
    role.addToPolicy(
      new iam.PolicyStatement({
        sid: 'AutoScaling',
        effect: iam.Effect.ALLOW,
        actions: [
          'application-autoscaling:RegisterScalableTarget',
          'application-autoscaling:DeregisterScalableTarget',
          'application-autoscaling:DescribeScalableTargets',
          'application-autoscaling:PutScalingPolicy',
          'application-autoscaling:DeleteScalingPolicy',
          'application-autoscaling:DescribeScalingPolicies',
        ],
        resources: ['*'],
      })
    );

    this.roleArn = role.roleArn;

    // Output the role ARN
    new cdk.CfnOutput(this, 'RoleArn', {
      value: role.roleArn,
      description: 'ARN of the GitHub Actions role for scorekeeper',
      exportName: 'ScorekeeperGitHubActionsRoleArn',
    });
  }
}
