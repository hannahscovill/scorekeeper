# Scorekeeper Deployment Runbook

This document provides step-by-step procedures for deploying, monitoring, and rolling back the Scorekeeper API service.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Building the Release Binary](#building-the-release-binary)
3. [Environment Configuration](#environment-configuration)
4. [CDK Infrastructure Overview](#cdk-infrastructure-overview)
5. [Deployment Procedures](#deployment-procedures)
6. [Health Check Verification](#health-check-verification)
7. [Rollback Procedures](#rollback-procedures)
8. [Common Troubleshooting](#common-troubleshooting)

---

## Prerequisites

### Local Development Requirements

- Rust toolchain (stable, 1.83+)
- Docker (for containerized builds)
- Node.js 20+ (for CDK operations)
- AWS CLI v2 configured with appropriate credentials

### AWS Requirements

- AWS account with CDK bootstrapped (`cdk bootstrap`)
- GitHub OIDC provider configured (see `infra/lib/github-oidc-stack.ts`)
- ECR repository created via `PrerequisiteInfraStack`

### GitHub Repository Variables

Configure these in Settings > Secrets and variables > Actions > Variables:

| Variable | Description | Example |
|----------|-------------|---------|
| `AWS_ACCOUNT_ID` | AWS account ID | `123456789012` |
| `AWS_REGION` | AWS region for deployment | `us-west-2` |
| `AWS_OIDC_ROLE_ARN` | IAM role ARN for OIDC auth | `arn:aws:iam::123456789012:role/GitHubActions-scorekeeper` |

---

## Building the Release Binary

### Local Build

```bash
# Build release binary
cargo build --release

# Binary location
./target/release/scorekeeper

# Run tests before building
cargo test --verbose
```

### Docker Build

```bash
# Build Docker image locally
docker build -t scorekeeper:local .

# Run locally for testing
docker run -p 8080:8080 \
  -e JWT_SECRET="your-test-secret" \
  -e RUST_LOG=debug \
  scorekeeper:local
```

### CI/CD Build

The CI workflow (`.github/workflows/ci.yml`) automatically:

1. Builds the project on every push/PR
2. Runs all tests
3. Caches cargo dependencies for faster builds

---

## Environment Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `HOST` | No | `0.0.0.0` | Host address to bind to |
| `PORT` | No | `8080` | Port to listen on |
| `DATABASE_URL` | No | None | Database connection string |
| `JWT_SECRET` | Yes (prod) | `development-secret-change-in-production` | JWT signing secret |
| `RUST_LOG` | No | `info` | Log level (debug, info, warn, error) |
| `ENVIRONMENT` | No | `dev` | Environment name |

### Environment-Specific Settings

#### Development

```bash
export RUST_LOG=debug
export JWT_SECRET="development-secret"
```

#### Staging

```bash
export RUST_LOG=info
export JWT_SECRET="<staging-secret-from-secrets-manager>"
export ENVIRONMENT=staging
```

#### Production

```bash
export RUST_LOG=info
export JWT_SECRET="<production-secret-from-secrets-manager>"
export ENVIRONMENT=prod
```

---

## CDK Infrastructure Overview

### Stack Architecture

The infrastructure is organized into three CDK stacks:

1. **GitHubOidcStack** - Bootstrap stack for GitHub Actions authentication
2. **PrerequisiteInfraStack** - Shared infrastructure (ECR repository)
3. **ScorekeeperStack** - Main application infrastructure (VPC, ECS, ALB)

### Infrastructure Components

```
                    ┌─────────────────────┐
                    │   Application       │
                    │   Load Balancer     │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   Target Group      │
                    │   (health: /health) │
                    └──────────┬──────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
     ┌────────▼────────┐ ┌─────▼──────┐ ┌──────▼───────┐
     │  Fargate Task   │ │  Fargate   │ │   Fargate    │
     │  (container)    │ │  Task ...  │ │   Task ...   │
     └─────────────────┘ └────────────┘ └──────────────┘
              │
     ┌────────▼────────┐
     │   ECR Image     │
     │   (scorekeeper) │
     └─────────────────┘
```

### Resource Naming Convention

Resources are named with environment suffix:
- `scorekeeper-vpc-{env}`
- `scorekeeper-cluster-{env}`
- `scorekeeper-service-{env}`

---

## Deployment Procedures

### Automated Deployment (Recommended)

Deployments are triggered automatically when:

1. Code is pushed to `main` branch
2. CI workflow passes successfully

The deploy workflow (`.github/workflows/deploy.yml`) then:

1. Builds and pushes Docker image to ECR
2. Deploys CDK stack changes
3. Forces new ECS deployment
4. Waits for service stabilization

### Manual Deployment

#### Step 1: Bootstrap Prerequisites (One-time)

```bash
cd infra

# Install dependencies
npm ci

# Deploy GitHub OIDC stack (requires GitHub org)
npx cdk deploy -c bootstrap=true -c githubOrg=<your-org> GitHubOidcStack

# Deploy prerequisite infrastructure (ECR repository)
npx cdk deploy -c prerequisite=true PrerequisiteInfraStack
```

#### Step 2: Build and Push Docker Image

```bash
# Login to ECR
aws ecr get-login-password --region us-west-2 | \
  docker login --username AWS --password-stdin <account-id>.dkr.ecr.us-west-2.amazonaws.com

# Build image
docker build -t scorekeeper:latest .

# Tag image
docker tag scorekeeper:latest \
  <account-id>.dkr.ecr.us-west-2.amazonaws.com/scorekeeper:latest

# Push image
docker push <account-id>.dkr.ecr.us-west-2.amazonaws.com/scorekeeper:latest
```

#### Step 3: Deploy CDK Stack

```bash
cd infra

# Deploy to dev
npx cdk deploy -c env=dev

# Deploy to staging
npx cdk deploy -c env=staging

# Deploy to production
npx cdk deploy -c env=prod --require-approval never
```

#### Step 4: Force New Deployment

```bash
# Force ECS to pull new image
aws ecs update-service \
  --cluster scorekeeper-cluster-prod \
  --service scorekeeper-service-prod \
  --force-new-deployment \
  --region us-west-2

# Wait for stabilization
aws ecs wait services-stable \
  --cluster scorekeeper-cluster-prod \
  --services scorekeeper-service-prod \
  --region us-west-2
```

---

## Health Check Verification

### Endpoint Verification

```bash
# Get the load balancer DNS from CDK outputs or AWS console
ALB_DNS="<load-balancer-dns>"

# Check health endpoint
curl -v http://${ALB_DNS}/health
# Expected: 200 OK, body: "OK"

# Check root endpoint
curl -v http://${ALB_DNS}/
# Expected: 200 OK, body: "Hello, World!"
```

### AWS Console Verification

1. **ECS Console**: Check service shows "Running" tasks at desired count
2. **Target Group**: Verify targets are "healthy" in EC2 > Target Groups
3. **CloudWatch Logs**: Check `/ecs/scorekeeper-{env}` for application logs

### CLI Health Checks

```bash
# Check ECS service status
aws ecs describe-services \
  --cluster scorekeeper-cluster-prod \
  --services scorekeeper-service-prod \
  --query 'services[0].{Status:status,Running:runningCount,Desired:desiredCount}'

# Check target health
aws elbv2 describe-target-health \
  --target-group-arn <target-group-arn> \
  --query 'TargetHealthDescriptions[*].{Target:Target.Id,Health:TargetHealth.State}'
```

---

## Rollback Procedures

### Automatic Rollback

The ECS service is configured with circuit breaker rollback enabled. If a deployment fails health checks, ECS automatically rolls back to the previous stable deployment.

### Manual Rollback via ECS

#### Option 1: Redeploy Previous Image Tag

```bash
# List recent image tags
aws ecr describe-images \
  --repository-name scorekeeper \
  --query 'imageDetails[*].{Tag:imageTags[0],Pushed:imagePushedAt}' \
  --output table

# Update service to use previous tag
# 1. Create new task definition with previous image tag
# 2. Update service to use new task definition
aws ecs update-service \
  --cluster scorekeeper-cluster-prod \
  --service scorekeeper-service-prod \
  --task-definition <previous-task-definition-arn>
```

#### Option 2: Force Rollback to Previous Task Definition

```bash
# List task definition revisions
aws ecs list-task-definitions \
  --family-prefix ScorekeeperStack \
  --sort DESC \
  --max-items 5

# Update service to previous revision
aws ecs update-service \
  --cluster scorekeeper-cluster-prod \
  --service scorekeeper-service-prod \
  --task-definition <previous-task-definition>
```

### CDK Rollback

```bash
cd infra

# View CloudFormation events for issues
aws cloudformation describe-stack-events \
  --stack-name ScorekeeperStack-prod \
  --query 'StackEvents[?ResourceStatus==`CREATE_FAILED` || ResourceStatus==`UPDATE_FAILED`]'

# Rollback to previous CDK deployment
# Option 1: Revert code and redeploy
git revert HEAD
npx cdk deploy -c env=prod

# Option 2: Use CloudFormation rollback
aws cloudformation rollback-stack \
  --stack-name ScorekeeperStack-prod
```

### Emergency Rollback

For critical issues, scale down to zero then back up:

```bash
# Scale to zero
aws ecs update-service \
  --cluster scorekeeper-cluster-prod \
  --service scorekeeper-service-prod \
  --desired-count 0

# Wait for tasks to drain
aws ecs wait services-stable \
  --cluster scorekeeper-cluster-prod \
  --services scorekeeper-service-prod

# Scale back up (will use current task definition)
aws ecs update-service \
  --cluster scorekeeper-cluster-prod \
  --service scorekeeper-service-prod \
  --desired-count 2
```

---

## Common Troubleshooting

### Deployment Fails to Start

**Symptom**: ECS tasks fail to start, service stuck at 0 running tasks

**Checks**:

```bash
# Check stopped tasks for error messages
aws ecs list-tasks \
  --cluster scorekeeper-cluster-prod \
  --desired-status STOPPED \
  --query 'taskArns[0:5]'

aws ecs describe-tasks \
  --cluster scorekeeper-cluster-prod \
  --tasks <task-arn> \
  --query 'tasks[*].{StopCode:stopCode,StopReason:stoppedReason}'
```

**Common Causes**:
- ECR image pull failure (check IAM permissions)
- Container crash on startup (check CloudWatch logs)
- Insufficient memory/CPU (increase task resources)

### Health Check Failures

**Symptom**: Tasks start but immediately become unhealthy

**Checks**:

```bash
# Check application logs
aws logs tail /ecs/scorekeeper-prod --follow

# Verify security group allows ALB health checks
aws ec2 describe-security-groups \
  --group-ids <task-security-group-id>
```

**Common Causes**:
- Application not listening on port 8080
- `/health` endpoint not responding
- Security group blocking ALB traffic
- Application startup timeout (default 60s)

### Image Pull Errors

**Symptom**: `CannotPullContainerError` in task events

**Checks**:

```bash
# Verify image exists
aws ecr describe-images \
  --repository-name scorekeeper \
  --image-ids imageTag=latest

# Check task execution role permissions
aws iam get-role-policy \
  --role-name <task-execution-role> \
  --policy-name <policy-name>
```

**Solutions**:
- Ensure ECR repository exists (`PrerequisiteInfraStack` deployed)
- Verify task execution role has `ecr:GetDownloadUrlForLayer` permission
- Check VPC has NAT gateway for private subnet ECR access

### CDK Deploy Failures

**Symptom**: `cdk deploy` fails with CloudFormation errors

**Checks**:

```bash
# View detailed stack events
aws cloudformation describe-stack-events \
  --stack-name ScorekeeperStack-prod \
  --max-items 20

# Check for resource conflicts
aws cloudformation describe-stack-resources \
  --stack-name ScorekeeperStack-prod
```

**Common Causes**:
- Resource already exists outside CDK
- IAM permission denied
- VPC availability zone lookup failure (ensure correct region)
- Concurrent deployment in progress (use `concurrency` group)

### Service Not Reachable

**Symptom**: Cannot connect to application via ALB DNS

**Checks**:

```bash
# Get ALB DNS
aws elbv2 describe-load-balancers \
  --query 'LoadBalancers[?contains(LoadBalancerName, `scorekeeper`)].DNSName'

# Check ALB security group
aws ec2 describe-security-groups \
  --filters "Name=group-name,Values=*scorekeeper*" \
  --query 'SecurityGroups[*].{ID:GroupId,Ingress:IpPermissions}'

# Verify listener configuration
aws elbv2 describe-listeners \
  --load-balancer-arn <alb-arn>
```

**Solutions**:
- Ensure ALB security group allows inbound traffic on port 80
- Verify target group has healthy targets
- Check DNS propagation if using custom domain

### High Memory/CPU Usage

**Symptom**: Tasks being killed due to OOM or high CPU throttling

**Checks**:

```bash
# View CloudWatch Container Insights (if enabled)
# Or check task metrics
aws cloudwatch get-metric-statistics \
  --namespace AWS/ECS \
  --metric-name MemoryUtilization \
  --dimensions Name=ClusterName,Value=scorekeeper-cluster-prod \
               Name=ServiceName,Value=scorekeeper-service-prod \
  --start-time $(date -u -v-1H +%Y-%m-%dT%H:%M:%SZ) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%SZ) \
  --period 300 \
  --statistics Average
```

**Solutions**:
- Increase task memory/CPU in CDK stack
- Review application for memory leaks
- Enable auto-scaling (already configured for prod)

---

## Additional Resources

- [AWS CDK Documentation](https://docs.aws.amazon.com/cdk/latest/guide/)
- [Amazon ECS Troubleshooting](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/troubleshooting.html)
- [Fargate Task Configuration](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_definition_parameters.html)
- Project infrastructure code: `infra/lib/infra-stack.ts`
- GitHub Actions workflows: `.github/workflows/`
