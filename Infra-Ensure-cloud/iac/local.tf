locals {
  taskdef_files = fileset("${path.module}/taskdefs", "*.json.template")
}

locals {
  ecs_services = {
    "headend-gateway" = {
      desired_count          = 1
      task_definition_key    = "headend-gateway"
      enable_execute_command = true
      launch_type            = "FARGATE"
      subnets                = var.private_subnets
      security_groups        = [aws_security_group.ecs_tasks.id]
      assign_public_ip       = false
      cpu                    = "512"
      memory                 = "1024"
      volumes                = []
      load_balancer = [
        {
          target_group_arn = aws_lb_target_group.headend_gateway.arn
          container_name   = "headend-gateway"
          container_port   = 3000
        }
      ]
    }
    "pki-api" = {
      desired_count          = 1
      task_definition_key    = "pki-api"
      enable_execute_command = true
      launch_type            = "FARGATE"
      subnets                = var.private_subnets
      security_groups        = [aws_security_group.ecs_tasks.id]
      assign_public_ip       = false
      cpu                    = "512"
      memory                 = "1024"
      volumes = [
        {
          name            = "stepca-efs"
          file_system_id  = var.stepca_efs_file_system_id
          access_point_id = var.stepca_efs_access_point_id
        }
      ]
      load_balancer = []
    }
    "stepca" = {
      desired_count          = 1
      task_definition_key    = "stepca"
      enable_execute_command = true
      launch_type            = "FARGATE"
      subnets                = var.private_subnets
      security_groups        = [aws_security_group.ecs_tasks.id]
      assign_public_ip       = false
      cpu                    = "512"
      memory                 = "1024"
      volumes = [
        {
          name            = "stepca-efs"
          file_system_id  = var.stepca_efs_file_system_id
          access_point_id = var.stepca_efs_access_point_id
        }
      ]
      load_balancer = []
    }
  }
}

locals {
  ecs_services_without_stepca = {
    for name, cfg in local.ecs_services : name => cfg
    if !contains(["stepca", "pki-api"], name)
  }
}

locals {
  ecs_log_group_names = [
    for name in keys(local.ecs_services) : "${var.environment}-${name}"
  ]
}

locals {
  pipeline_name_effective = var.pipeline_name != "" ? var.pipeline_name : "${var.project}-${var.environment}-master-pipeline"

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
      name  = "HEADEND_GATEWAY_IMAGE_REPO_NAME"
      value = var.headend_gateway_image_repo_name
      type  = "PLAINTEXT"
    },
    {
      name  = "PKI_API_IMAGE_REPO_NAME"
      value = var.pki_api_image_repo_name
      type  = "PLAINTEXT"
    },
    {
      name  = "STEPCA_IMAGE_REPO_NAME"
      value = var.stepca_image_repo_name
      type  = "PLAINTEXT"
    }
  ])
}
