locals {
  name_prefix = lower(replace("${var.project_name}-${var.environment}", "_", "-"))

  merged_tags = merge(
    {
      Project     = var.project_name
      Environment = var.environment
      ManagedBy   = "terraform"
    },
    var.tags
  )

  private_subnet_ids = [
    data.aws_subnet.private_1.id,
    data.aws_subnet.private_2.id
  ]
}

locals {
  build_action_env_vars = jsonencode([
    {
      name  = "ENVIRONMENT"
      value = var.environment
      type  = "PLAINTEXT"
    },
    {
      name  = "RELEASE_SHA"
      value = "#{AppSourceVars.CommitId}"
      type  = "PLAINTEXT"
    }
  ])

  security_action_env_vars = jsonencode([
    {
      name  = "ENVIRONMENT"
      value = var.environment
      type  = "PLAINTEXT"
    },
    {
      name  = "RELEASE_SHA"
      value = "#{AppSourceVars.CommitId}"
      type  = "PLAINTEXT"
    }
  ])

  deploy_action_env_vars = jsonencode([
    {
      name  = "ENVIRONMENT"
      value = var.environment
      type  = "PLAINTEXT"
    },
    {
      name  = "BUCKET_NAME"
      value = var.artifact_bucket_name
      type  = "PLAINTEXT"
    },
    {
      name  = "RELEASE_SHA"
      value = "#{AppSourceVars.CommitId}"
      type  = "PLAINTEXT"
    },
    {
      name  = "YARDI_IMAGE_REPO_NAME"
      value = var.yardi_image_repo_name
      type  = "PLAINTEXT"
    }
  ])
}
