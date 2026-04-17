# =============================================================================
# Sentrics-Core ECS
# =============================================================================

module "ecs_cluster" {
  source      = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster?ref=develop2-clean-asg-elb"
  cluster_map = var.cluster_map
  environment = var.environment
}

resource "aws_ecs_task_definition" "yardi_sync" {
  family                   = "${var.ecs_task_definition_family}-${var.environment}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.ecs_task_cpu
  memory                   = var.ecs_task_memory
  execution_role_arn       = var.ecs_task_role_arn
  task_role_arn            = var.ecs_task_role_arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "ARM64"
  }

  container_definitions = templatefile("${path.module}/${var.ecs_task_definition_file}", {
    ecs_container_image = var.ecs_container_image
  })
}

module "ecs_service" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/Ecs-service?ref=develop2-clean-asg-elb"

  service_name           = "${var.project_name}-${var.environment}-yardi-sync-service"
  cluster_id             = module.ecs_cluster.cluster_arns["main"]
  task_definition_arn    = aws_ecs_task_definition.yardi_sync.arn
  desired_count          = var.ecs_service_desired_count
  enable_execute_command = var.ecs_service_enable_execute_command

  availability_zone_rebalancing      = "DISABLED"
  deployment_minimum_healthy_percent = 0
  deployment_maximum_percent         = 100

  subnets          = var.subnets
  security_groups  = var.ecs_security_groups
  assign_public_ip = var.ecs_service_assign_public_ip

  environment = var.environment
  project     = var.project_name
  owner       = lookup(var.tags, "Owner", "")

  depends_on = [module.ecs_logs]
}

module "ecs_logs" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/ecs-logs?ref=develop2-clean-asg-elb"

  services          = var.ecs_services
  retention_in_days = var.log_retention
  tags              = var.tags
}

# =============================================================================
# Ensure-Cloud ECS
# (Terraform label ec_ecs_cluster avoids conflict with sentrics-core module "ecs_cluster";
#  the AWS cluster name is controlled by var.ec_cluster_map and is unchanged)
# =============================================================================

module "ec_ecs_cluster" {
  source      = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster?ref=develop2-clean-asg-elb"
  cluster_map = var.ec_cluster_map
  environment = var.environment
}

resource "aws_ecs_task_definition" "ecs_task_definitions" {
  for_each = {
    for f in local.taskdef_files :
    replace(basename(f), ".json.template", "") => {
      family                        = "${var.project}-${var.environment}-${replace(basename(f), ".json.template", "")}-task"
      task_definition_template_file = "${path.module}/taskdefs/${f}"
      cpu     = try(local.ecs_services[replace(basename(f), ".json.template", "")].cpu, "512")
      memory  = try(local.ecs_services[replace(basename(f), ".json.template", "")].memory, "1024")
      volumes = try(local.ecs_services[replace(basename(f), ".json.template", "")].volumes, [])
    }
  }

  family                   = each.value.family
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = each.value.cpu
  memory                   = each.value.memory
  execution_role_arn       = var.ecs_execution_role_arn
  task_role_arn            = var.ecs_task_role_arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "ARM64"
  }

  container_definitions = templatefile(each.value.task_definition_template_file, {
    ecs_container_image = var.ecs_task_images[each.key]
    ecs_log_group       = "/ecs/${var.environment}-${each.key}"
  })

  dynamic "volume" {
    for_each = each.value.volumes
    content {
      name = volume.value.name
      efs_volume_configuration {
        file_system_id     = volume.value.file_system_id
        transit_encryption = "ENABLED"
        authorization_config {
          access_point_id = volume.value.access_point_id
          iam             = "DISABLED"
        }
      }
    }
  }
}

module "Ecs-service" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/Ecs-service?ref=develop2-clean-asg-elb"

  for_each = local.ecs_services_without_stepca

  service_name           = "${var.project}-${var.environment}-${each.key}-service"
  cluster_id             = module.ec_ecs_cluster.cluster_arns["main"]
  task_definition_arn    = aws_ecs_task_definition.ecs_task_definitions[each.value.task_definition_key].arn
  desired_count          = each.value.desired_count
  enable_execute_command = each.value.enable_execute_command

  subnets          = each.value.subnets
  security_groups  = each.value.security_groups
  assign_public_ip = each.value.assign_public_ip

  load_balancer = try(each.value.load_balancer, [])

  environment = var.environment
  project     = var.project
  owner       = var.owner

  depends_on = [
    module.ec_ecs_logs,
    aws_lb_listener.headend_gateway_https
  ]
}

resource "aws_ecs_service" "pki_api" {
  name                   = "${var.project}-${var.environment}-pki-api-service"
  cluster                = module.ec_ecs_cluster.cluster_arns["main"]
  task_definition        = aws_ecs_task_definition.ecs_task_definitions["pki-api"].arn
  desired_count          = local.ecs_services["pki-api"].desired_count
  launch_type            = "FARGATE"
  enable_execute_command = local.ecs_services["pki-api"].enable_execute_command

  service_registries {
    registry_arn = aws_service_discovery_service.pki_api.arn
  }

  network_configuration {
    subnets          = local.ecs_services["pki-api"].subnets
    security_groups  = local.ecs_services["pki-api"].security_groups
    assign_public_ip = local.ecs_services["pki-api"].assign_public_ip
  }

  deployment_minimum_healthy_percent = 50
  deployment_maximum_percent         = 200

  tags = merge({ Environment = var.environment, Project = var.project, Owner = var.owner }, var.tags)

  depends_on = [
    module.ec_ecs_logs,
    aws_ecs_service.stepca
  ]
}

resource "aws_ecs_service" "stepca" {
  name                          = "${var.project}-${var.environment}-stepca-service"
  cluster                       = module.ec_ecs_cluster.cluster_arns["main"]
  task_definition               = aws_ecs_task_definition.ecs_task_definitions["stepca"].arn
  desired_count                 = local.ecs_services["stepca"].desired_count
  launch_type                   = "FARGATE"
  enable_execute_command        = local.ecs_services["stepca"].enable_execute_command
  availability_zone_rebalancing = "DISABLED"

  service_registries {
    registry_arn = aws_service_discovery_service.stepca.arn
  }

  network_configuration {
    subnets          = local.ecs_services["stepca"].subnets
    security_groups  = local.ecs_services["stepca"].security_groups
    assign_public_ip = local.ecs_services["stepca"].assign_public_ip
  }

  deployment_minimum_healthy_percent = 0
  deployment_maximum_percent         = 100

  tags = merge({ Environment = var.environment, Project = var.project, Owner = var.owner }, var.tags)

  depends_on = [module.ec_ecs_logs]
}

module "ec_ecs_logs" {
  source            = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/ecs-logs?ref=develop2-clean-asg-elb"
  services          = local.ecs_log_group_names
  retention_in_days = var.log_retention
  tags              = var.tags
}

module "autoscaling" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/autoscaling?ref=develop2-clean-asg-elb"

  service_namespace  = "ecs"
  resource_id        = "service/${var.ec_cluster_map["main"].name}/${module.Ecs-service["headend-gateway"].service_name}"
  scalable_dimension = "ecs:service:DesiredCount"

  min_capacity = var.min_capacity
  max_capacity = var.max_capacity

  cpu_target_value    = var.cpu_target_value
  memory_target_value = var.memory_target_value

  scale_in_cooldown  = var.scale_in_cooldown
  scale_out_cooldown = var.scale_out_cooldown

  tags       = var.tags
  depends_on = [module.Ecs-service["headend-gateway"]]
}
