import * as cdk from 'aws-cdk-lib/core';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Construct } from 'constructs';

export interface GitHubOidcStackProps extends cdk.StackProps {
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

export class GitHubOidcStack extends cdk.Stack {
  public readonly oidcRoleArn: cdk.CfnOutput;

  constructor(scope: Construct, id: string, props: GitHubOidcStackProps) {
    super(scope, id, props);

    const { githubOrg, githubRepo, allowedBranches = ['main'] } = props;

    // Create the OIDC provider for GitHub Actions
    // Note: This may already exist in the account - CDK handles this gracefully
    const githubOidcProvider = new iam.OpenIdConnectProvider(
      this,
      'GitHubOidcProvider',
      {
        url: 'https://token.actions.githubusercontent.com',
        clientIds: ['sts.amazonaws.com'],
        thumbprints: [
          // GitHub's OIDC thumbprint - this is a well-known value
          '6938fd4d98bab03faadb97b34396831e3780aea1',
          '1c58a3a8518e8759bf075b76b750d4f2df264fcd',
        ],
      }
    );

    // Build the subject claim conditions for allowed branches
    const subjectConditions: string[] = allowedBranches.map(
      (branch) => `repo:${githubOrg}/${githubRepo}:ref:refs/heads/${branch}`
    ).concat([`repo:${githubOrg}/${githubRepo}:*`]); // maybe go back to just main??

    // Create the IAM role that GitHub Actions will assume
    const githubActionsRole = new iam.Role(this, 'GitHubActionsRole', {
      roleName: `GitHubActions-${githubRepo}`,
      description: `Role for GitHub Actions to deploy ${githubRepo}`,
      maxSessionDuration: cdk.Duration.hours(1),
      assumedBy: new iam.WebIdentityPrincipal(
        githubOidcProvider.openIdConnectProviderArn,
        {
          StringEquals: {
            'token.actions.githubusercontent.com:aud': 'sts.amazonaws.com',
          },
          StringLike: {
            'token.actions.githubusercontent.com:sub': subjectConditions,
          },
        }
      ),
    });

    // ECR permissions - push and pull images
    githubActionsRole.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECRAuth',
        effect: iam.Effect.ALLOW,
        actions: ['ecr:GetAuthorizationToken'],
        resources: ['*'],
      })
    );

    githubActionsRole.addToPolicy(
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

    // ECR permissions for creating/managing the prerequisite repository
    githubActionsRole.addToPolicy(
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

    // CloudFormation permissions for CDK deployments
    githubActionsRole.addToPolicy(
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

    // CloudFormation read permissions (needed for CDK to get detailed errors)
    githubActionsRole.addToPolicy(
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
    githubActionsRole.addToPolicy(
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

    // Additional ECS permission for service stabilization
    githubActionsRole.addToPolicy(
      new iam.PolicyStatement({
        sid: 'ECSWait',
        effect: iam.Effect.ALLOW,
        actions: ['ecs:DescribeTasks', 'ecs:ListTasks'],
        resources: ['*'],
      })
    );

    // EC2/VPC permissions for CDK infrastructure
    githubActionsRole.addToPolicy(
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

    // Elastic Load Balancing permissions
    githubActionsRole.addToPolicy(
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

    // IAM permissions (limited for CDK role creation)
    githubActionsRole.addToPolicy(
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

    // STS permissions to assume CDK bootstrap roles
    githubActionsRole.addToPolicy(
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

    // S3 permissions for CDK asset bucket
    githubActionsRole.addToPolicy(
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

    // S3 permissions for avatar bucket (created by PrerequisiteInfraStack)
    githubActionsRole.addToPolicy(
      new iam.PolicyStatement({
        sid: 'S3AvatarBucket',
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
        ],
        resources: [
          'arn:aws:s3:::scorekeeper-avatars',
        ],
      })
    );

    // SSM Parameter Store (CDK bootstrap version)
    githubActionsRole.addToPolicy(
      new iam.PolicyStatement({
        sid: 'SSM',
        effect: iam.Effect.ALLOW,
        actions: ['ssm:GetParameter', 'ssm:GetParameters'],
        resources: [
          `arn:aws:ssm:*:${this.account}:parameter/cdk-bootstrap/*`,
        ],
      })
    );

    // CloudWatch Logs permissions
    githubActionsRole.addToPolicy(
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

    // Application Auto Scaling permissions
    githubActionsRole.addToPolicy(
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

    // Output the role ARN for use in GitHub secrets
    this.oidcRoleArn = new cdk.CfnOutput(this, 'GitHubActionsRoleArn', {
      value: githubActionsRole.roleArn,
      description: 'ARN of the IAM role for GitHub Actions OIDC authentication',
      exportName: 'GitHubActionsOidcRoleArn',
    });

    new cdk.CfnOutput(this, 'OidcProviderArn', {
      value: githubOidcProvider.openIdConnectProviderArn,
      description: 'ARN of the GitHub OIDC identity provider',
    });
  }
}
