# Scorekeeper Infrastructure

CDK stacks for the scorekeeper backend service.

## Stacks

| Stack | Command | Purpose |
|-------|---------|---------|
| `PrerequisiteInfraStack` | `-c prerequisite=true` | ECR repository for Docker images |
| `ScorekeeperStack-{env}` | `-c env=prod` | Main app: ECS Fargate, DynamoDB, ALB, VPC |
| `ScorekeeperGitHubActionsRoleStack` | `-c role=true -c githubOrg=hannahscovill` | IAM role for GitHub Actions CI/CD |

### GitHub Actions Role

The role stack imports `GitHubOidcProviderArn` from [shared-infrastructure](https://github.com/hannahscovill/shared-infrastructure). The shared OIDC provider must be deployed first.

Permissions include: ECR, ECS, CloudFormation, VPC, ELB, IAM, S3, DynamoDB, Secrets Manager, CloudWatch Logs, Auto Scaling, and Terraform state access (S3 + DynamoDB).

## Deploying

```bash
cd infra
npm install

# Prerequisites (once)
npx cdk deploy -c prerequisite=true PrerequisiteInfraStack

# GitHub Actions role (once, or when permissions change)
npx cdk deploy -c role=true -c githubOrg=hannahscovill ScorekeeperGitHubActionsRoleStack

# Main application
npx cdk deploy -c env=prod \
  -c auth0M2mSecretArn=<ARN>
```

## CI/CD

Deployment is automated via GitHub Actions (`.github/workflows/deploy.yml`). Required secrets:

| Secret | Description |
|--------|-------------|
| `AWS_REGION` | AWS region (e.g., `us-west-2`) |
| `AUTH0_M2M_SECRET_ARN` | Secrets Manager ARN for Auth0 M2M credentials |
