#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib/core';
import { ScorekeeperStack } from '../lib/infra-stack';
import { PrerequisiteInfraStack } from '../lib/prerequisite-infra-stack';
import { GitHubActionsRoleStack } from '../lib/github-actions-role-stack';

const app = new cdk.App();

const githubOrg = app.node.tryGetContext('githubOrg');

new PrerequisiteInfraStack(app, 'PrerequisiteInfraStack', {
  repositoryName: 'scorekeeper',
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION || 'us-west-2',
  },
});

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
});

new ScorekeeperStack(app, 'ScorekeeperStack', {
  environmentName: 'prod',
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region: process.env.CDK_DEFAULT_REGION,
  },
});
