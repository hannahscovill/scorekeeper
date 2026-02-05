# Scorekeeper Infrastructure

CDK stacks for the scorekeeper backend service.

## Stacks

| Stack | Purpose |
|-------|---------|
| `PrerequisiteInfraStack` | ECR repository, Route53 hosted zone |
| `ScorekeeperStack` | Main app: ECS Fargate, DynamoDB, S3, ALB, VPC, OTel collector sidecar |
| `ScorekeeperGitHubActionsRoleStack` | IAM role for GitHub Actions CI/CD |

### GitHub Actions Role

The role stack imports `GitHubOidcProviderArn` from [shared-infrastructure](https://github.com/hannahscovill/shared-infrastructure). The shared OIDC provider must be deployed first.

Permissions include: ECR, ECS, CloudFormation, VPC, ELB, IAM, S3, DynamoDB, Secrets Manager, CloudWatch Logs, Auto Scaling, and Terraform state access (S3 + DynamoDB).

## Deploying

```bash
cd infra
npm install

# Prerequisites (once)
npx cdk deploy PrerequisiteInfraStack

# GitHub Actions role (once, or when permissions change)
npx cdk deploy ScorekeeperGitHubActionsRoleStack

# Main application
npx cdk deploy ScorekeeperStack -c env=prod \
  -c auth0M2mSecretArn=<ARN> \
  -c otelSecretsArn=<ARN>
```

## CI/CD

Deployment is automated via GitHub Actions (`.github/workflows/deploy.yml`). Required secrets:

| Secret | Description |
|--------|-------------|
| `AWS_REGION` | AWS region (e.g., `us-west-2`) |
| `AUTH0_M2M_SECRET_ARN` | Secrets Manager ARN for Auth0 M2M credentials |
| `OTEL_SECRETS_ARN` | Secrets Manager ARN for Grafana Cloud OTLP credentials (optional) |

Grafana dashboards are provisioned separately via `.github/workflows/grafana-dashboards.yml` when `grafana/` files change.
