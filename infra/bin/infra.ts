#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib/core';
import { ScorekeeperStack } from '../lib/infra-stack';

const app = new cdk.App();

// Get environment from CDK context or environment variable
// Usage: cdk deploy -c env=prod
// Or: SCOREKEEPER_ENV=prod cdk deploy
const environmentName =
  app.node.tryGetContext('env') ||
  process.env.SCOREKEEPER_ENV ||
  'dev';

// Validate environment name
const validEnvironments = ['dev', 'staging', 'prod'];
if (!validEnvironments.includes(environmentName)) {
  throw new Error(
    `Invalid environment: ${environmentName}. Must be one of: ${validEnvironments.join(', ')}`
  );
}

// Create the stack with environment-specific naming
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

app.synth();
