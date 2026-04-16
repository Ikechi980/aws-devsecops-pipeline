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

