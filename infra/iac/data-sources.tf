# VPC and subnets — looked up by tag for sentrics-core resources
data "aws_vpc" "this" {
  filter {
    name   = "tag:Name"
    values = [var.vpc_name]
  }
}

data "aws_subnet" "private_1" {
  filter {
    name   = "tag:Name"
    values = ["Ensure-private-sub-1"]
  }
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.this.id]
  }
}

data "aws_subnet" "private_2" {
  filter {
    name   = "tag:Name"
    values = ["Ensure-private-sub-2"]
  }
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.this.id]
  }
}

# Shared Lambda assume-role policy document
data "aws_iam_policy_document" "lambda_assume_role" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}

# Used by ensure-cloud resources
data "aws_caller_identity" "current" {}

data "aws_route53_zone" "public" {
  name         = "${var.public_hosted_zone_name}."
  private_zone = false
}
