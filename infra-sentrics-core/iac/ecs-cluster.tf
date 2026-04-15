# ECS Cluster
module "ecs_cluster" {
  source      = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster?ref=develop2-clean-asg-elb"
  cluster_map = var.cluster_map
  environment = var.environment
}

# Single ECS Task Definition (yardi-sync)
resource "aws_ecs_task_definition" "yardi_sync" {
  family                   = "${var.ecs_task_definition_family}-${var.environment}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.ecs_task_cpu
  memory                   = var.ecs_task_memory
  execution_role_arn       = var.ecs_task_role_arn
  task_role_arn            = var.ecs_task_role_arn

  container_definitions = templatefile("${path.module}/${var.ecs_task_definition_file}", {
    ecs_container_image = var.ecs_container_image
  })
}

# Single ECS Service (yardi-sync)
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
}

# Ecs CloudWatch Log Group

module "ecs_logs" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/ecs-logs?ref=develop2-clean-asg-elb"

  services          = var.ecs_services
  retention_in_days = var.log_retention
  tags              = var.tags
}
