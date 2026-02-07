import * as cdk from 'aws-cdk-lib/core';
import { Template } from 'aws-cdk-lib/assertions';
import { ScorekeeperStack, ScorekeeperStackProps } from '../lib/infra-stack';

const testEnv = { account: '123456789012', region: 'us-west-2' };

function createStack(props?: Partial<ScorekeeperStackProps>): Template {
  const app = new cdk.App();
  const stack = new ScorekeeperStack(app, 'TestStack', {
    env: testEnv,
    ...props,
  });
  return Template.fromStack(stack);
}

describe('ScorekeeperStack', () => {
  test('references existing ECR repository', () => {
    const template = createStack();

    // ECR repository is created by PrerequisiteInfraStack, not this stack
    // Verify no ECR repository is created here
    template.resourceCountIs('AWS::ECR::Repository', 0);
  });

  test('creates VPC with 2 AZs', () => {
    const template = createStack();

    // VPC should exist
    template.hasResourceProperties('AWS::EC2::VPC', {});

    // Should have subnets (2 public + 2 private = 4 subnets for 2 AZs)
    template.resourceCountIs('AWS::EC2::Subnet', 4);
  });

  test('creates ECS cluster', () => {
    const template = createStack();

    template.hasResourceProperties('AWS::ECS::Cluster', {
      ClusterName: 'scorekeeper-cluster-prod',
    });
  });

  test('creates Fargate service with correct configuration', () => {
    const template = createStack();

    // Check task definition
    template.hasResourceProperties('AWS::ECS::TaskDefinition', {
      Cpu: '256',
      Memory: '512',
      RequiresCompatibilities: ['FARGATE'],
    });

    // Check service exists
    template.hasResourceProperties('AWS::ECS::Service', {
      ServiceName: 'scorekeeper-service-prod',
    });
  });

  test('creates Application Load Balancer', () => {
    const template = createStack();

    template.hasResourceProperties('AWS::ElasticLoadBalancingV2::LoadBalancer', {
      Scheme: 'internet-facing',
      Type: 'application',
    });
  });

  test('configures health check on /health endpoint', () => {
    const template = createStack();

    template.hasResourceProperties('AWS::ElasticLoadBalancingV2::TargetGroup', {
      HealthCheckPath: '/health',
      HealthCheckPort: '8080',
    });
  });

  test('enables auto-scaling', () => {
    const template = createStack();

    template.hasResourceProperties('AWS::ApplicationAutoScaling::ScalableTarget', {
      MinCapacity: 2,
      MaxCapacity: 10,
    });
  });

  test('creates 2 NAT gateways', () => {
    const template = createStack();
    template.resourceCountIs('AWS::EC2::NatGateway', 2);
  });
});
