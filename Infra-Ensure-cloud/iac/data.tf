data "aws_caller_identity" "current" {}

data "aws_route53_zone" "public" {
  name         = "${var.public_hosted_zone_name}."
  private_zone = false
}

data "terraform_remote_state" "sentrics_core" {
  backend = "s3"
  config = {
    bucket       = "sentrics-ensure-terraform-state-codepipeline-cache"
    key          = "${var.environment}/sentrics-core/terraform.tfstate"
    region       = var.region
    encrypt      = true
    use_lockfile = true
  }
}

data "aws_iam_policy_document" "lambda_assume" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}
