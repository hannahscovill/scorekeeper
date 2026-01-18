#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib/core';
import { ScorekeeperStack } from '../lib/infra-stack';
import { GitHubOidcStack } from '../lib/github-oidc-stack';

const app = new cdk.App();

// Get environment from CDK context or environment variable
// Usage: cdk deploy -c env=prod
// Or: SCOREKEEPER_ENV=prod cdk deploy
const environmentName =
  app.node.tryGetContext('env') ||
  process.env.SCOREKEEPER_ENV ||
  'dev';

// Check if we're deploying the bootstrap stack
// Usage: cdk deploy -c bootstrap=true GitHubOidcStack
const isBootstrap = app.node.tryGetContext('bootstrap') === 'true';

// GitHub configuration for OIDC stack
const githubOrg = app.node.tryGetContext('githubOrg') || process.env.GITHUB_ORG;
const githubRepo = app.node.tryGetContext('githubRepo') || process.env.GITHUB_REPO || 'scorekeeper';

// Validate environment name
const validEnvironments = ['dev', 'staging', 'prod'];
if (!validEnvironments.includes(environmentName)) {
  throw new Error(
    `Invalid environment: ${environmentName}. Must be one of: ${validEnvironments.join(', ')}`
  );
}

// Create the GitHub OIDC stack (prerequisite/bootstrap infrastructure)
// This stack creates the IAM role that GitHub Actions uses for OIDC authentication
// Deploy with: cdk deploy -c bootstrap=true -c githubOrg=your-org GitHubOidcStack
if (isBootstrap || githubOrg) {
  if (!githubOrg) {
    throw new Error(
      'GitHub organization is required for OIDC stack. Provide via -c githubOrg=your-org or GITHUB_ORG env var'
    );
  }

  new GitHubOidcStack(app, 'GitHubOidcStack', {
    githubOrg,
    githubRepo,
    allowedBranches: ['main'],
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region: process.env.CDK_DEFAULT_REGION,
    },
    tags: {
      Project: 'scorekeeper',
      ManagedBy: 'cdk',
      Purpose: 'github-oidc-bootstrap',
    },
  });
}

// Create the main Scorekeeper stack with environment-specific naming
// Skip this when in bootstrap mode to avoid VPC AZ lookups
if (!isBootstrap) {
  new ScorekeeperStack(app, `ScorekeeperStack-${environmentName}`, {
    environmentName,
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
