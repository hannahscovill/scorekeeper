#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib/core';
import { ScorekeeperStack } from '../lib/infra-stack';
import { GitHubOidcStack } from '../lib/github-oidc-stack';
import { PrerequisiteInfraStack } from '../lib/prerequisite-infra-stack';
import { GitHubActionsRoleStack } from '../lib/github-actions-role-stack';

const app = new cdk.App();

// Get environment from CDK context or environment variable
// Usage: cdk deploy -c env=prod
// Or: SCOREKEEPER_ENV=prod cdk deploy
const environmentName =
  app.node.tryGetContext('env') ||
  process.env.SCOREKEEPER_ENV ||
  'dev';

// Check if we're deploying the bootstrap/prerequisite stacks
// Usage: cdk deploy -c bootstrap=true GitHubOidcStack (DEPRECATED - use shared-infrastructure)
// Usage: cdk deploy -c prerequisite=true PrerequisiteInfraStack
const isBootstrap = app.node.tryGetContext('bootstrap') === 'true';
const isPrerequisite = app.node.tryGetContext('prerequisite') === 'true';

// GitHub configuration
const githubOrg = app.node.tryGetContext('githubOrg');

// Validate environment name
const validEnvironments = ['dev', 'staging', 'prod'];
if (!validEnvironments.includes(environmentName)) {
  throw new Error(
    `Invalid environment: ${environmentName}. Must be one of: ${validEnvironments.join(', ')}`
  );
}

// DEPRECATED: Use shared-infrastructure/GitHubOidcProviderStack instead
// Deploy with: cdk deploy -c bootstrap=true GitHubOidcStack
if (isBootstrap) {
  new GitHubOidcStack(app, 'GitHubOidcStack', {
    githubOrg,
    repos: [
      { repo: 'scorekeeper', branches: ['main'] },
      { repo: 'wordles-with-friends-client-web', branches: ['main'] },
    ],
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region: process.env.CDK_DEFAULT_REGION,
    },
    tags: {
      Project: 'orchestra',
      ManagedBy: 'cdk',
      Purpose: 'github-oidc-bootstrap',
    },
  });
}

// Create the Prerequisite Infrastructure stack (ECR repository)
// This stack creates shared infrastructure that must exist before deployment
// Deploy with: cdk deploy PrerequisiteInfraStack
// To import existing ECR: cdk import PrerequisiteInfraStack
if (isPrerequisite) {
  new PrerequisiteInfraStack(app, 'PrerequisiteInfraStack', {
    repositoryName: 'scorekeeper',
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region: process.env.CDK_DEFAULT_REGION || 'us-west-2',
    },
    tags: {
      Project: 'scorekeeper',
      ManagedBy: 'cdk',
      Purpose: 'prerequisite-infrastructure',
    },
  });
}

// SECURITY: Role stack must be deployed manually from a local machine.
// The GitHub Actions role does NOT have permissions to modify this stack.
new GitHubActionsRoleStack(app, 'ScorekeeperGitHubActionsRoleStack', {
  githubOrg,
  githubRepo: 'scorekeeper',
  allowedBranches: ['main'],
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION,
  },
  tags: {
    Project: 'scorekeeper',
    ManagedBy: 'cdk',
    Purpose: 'github-actions-role',
  },
});

// Create the main Scorekeeper stack with environment-specific naming
// Skip this when in bootstrap or prerequisite mode to avoid VPC AZ lookups
if (!isBootstrap && !isPrerequisite) {
  new ScorekeeperStack(app, `ScorekeeperStack-${environmentName}`, {
    environmentName,
    // Import existing S3 bucket (created outside CDK)
    importExistingAvatarBucket: true,
    // Use the current CLI configuration for account and region
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region: process.env.CDK_DEFAULT_REGION,
    },
    // Stack tags for resource organization
    tags: {
      Environment: environmentName,
      Project: 'scorekeeper',
      ManagedBy: 'cdk',
    },
  });
}

app.synth();
