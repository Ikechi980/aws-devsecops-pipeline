variable "region" {
  description = "AWS region"
  type        = string
}

variable "vpc_id" {
  description = "VPC ID"
  type        = string
  default     = "vpc-0d5f95e46a0aa79bb"
}

variable "private_subnets" {
  description = "List of private subnet IDs"
  type        = list(string)
  default     = ["subnet-0ceb4c6f3bf403042", "subnet-0d68a4043bdaaf613"]
}

variable "ecs_execution_role_arn" {
  description = "ECS task execution role ARN"
  type        = string
  default     = "arn:aws:iam::892234674906:role/ecs-prod-ecs-execution-role"
}

variable "ecs_task_role_arn" {
  description = "ECS task role ARN"
  type        = string
  default     = "arn:aws:iam::892234674906:role/ecs-prod-ecs-execution-role"
}

variable "stepca_efs_file_system_id" {
  description = "Existing EFS file system ID for StepCA persistent data"
  type        = string
}

variable "stepca_efs_access_point_id" {
  description = "Existing EFS access point ID for StepCA mount"
  type        = string
}

variable "lambda_s3_bucket" {
  description = "S3 bucket for Lambda artifacts"
  type        = string
  default     = "sentrics-ensure-lambda-artifacts-truststore"
}

variable "lambda_headend_api_s3_key" {
  description = "S3 key for headend-api zip"
  type        = string
  default     = "dev/lambda-artifacts/headend-api/headend-api.zip"
}

variable "lambda_core_change_publisher_s3_key" {
  description = "S3 key for core-change-publisher zip"
  type        = string
  default     = "dev/lambda-artifacts/core-change-publisher/core-change-publisher.zip"
}

variable "headend_api_systems_api_base_url" {
  description = "Base URL for ensure360-ems used by headend-api Lambda"
  type        = string
}

variable "headend_api_events_mongo_url_ssm_parameter" {
  description = "SSM parameter name containing the MongoDB connection URL for headend-api events endpoint"
  type        = string
  default     = "/ensure-cloud/headend-api/events-mongo-url"
}

variable "headend_api_events_limit_default" {
  description = "Default events query limit for headend-api Lambda"
  type        = number
  default     = 100
}

variable "headend_api_events_limit_max" {
  description = "Maximum events query limit for headend-api Lambda"
  type        = number
  default     = 1000
}

variable "headend_api_allow_unauthenticated" {
  description = "Enable local unauthenticated fallback in headend-api Lambda"
  type        = bool
  default     = false
}

variable "headend_api_rust_log" {
  description = "RUST_LOG for headend-api Lambda"
  type        = string
  default     = "info"
}

variable "core_change_publisher_systems_api_base_url" {
  description = "Base URL for ensure systems API used by core-change-publisher Lambda"
  type        = string
}

variable "core_change_publisher_aws_endpoint_url" {
  description = "Optional AWS endpoint override (for LocalStack) for core-change-publisher Lambda"
  type        = string
  default     = ""
}

variable "core_change_publisher_rust_log" {
  description = "RUST_LOG for core-change-publisher Lambda"
  type        = string
  default     = "info"
}

variable "alb_certificate_arn" {
  description = "ACM certificate ARN for ALB listener"
  type        = string
}

variable "alb_trust_store_bucket" {
  description = "S3 bucket for ALB trust store (CA bundle)"
  type        = string
}

variable "alb_trust_store_key" {
  description = "S3 key for ALB trust store (CA bundle)"
  type        = string
}

variable "alb_ingress_cidrs" {
  description = "CIDR blocks allowed to reach the ALB"
  type        = list(string)
  default     = []
}

variable "alb_custom_domain_name" {
  description = "Optional public DNS name for the headend-gateway ALB"
  type        = string
  default     = ""
}

variable "ecs_tasks_ingress_cidrs" {
  description = "CIDR blocks allowed to reach ECS tasks directly"
  type        = list(string)
  default     = []
}

variable "wg_ingress_cidrs" {
  description = "WireGuard CIDR blocks allowed to reach pki-api on 8080"
  type        = list(string)
  default     = []
}

variable "apigw_custom_domain_name" {
  description = "Custom domain for API Gateway HTTP API with mTLS"
  type        = string
}

variable "apigw_certificate_arn" {
  description = "ACM certificate ARN for API Gateway custom domain"
  type        = string
}

variable "apigw_trust_store_bucket" {
  description = "S3 bucket for API Gateway mTLS trust store"
  type        = string
}

variable "apigw_trust_store_key" {
  description = "S3 key for API Gateway mTLS trust store"
  type        = string
}

variable "public_hosted_zone_name" {
  description = "Public Route 53 hosted zone name used for ALB and API custom-domain aliases"
  type        = string
  default     = "ensurelink.net"
}


variable "project" {
  description = "Project name"
  type        = string
}

variable "environment" {
  description = "Environment name (e.g., dev, staging, prod)"
  type        = string
}

variable "resource_version" {
  description = "Resource version tag"
  type        = string
}

variable "owner" {
  description = "Owner of the project"
  type        = string
}

variable "cluster_map" {
  description = "Map of ECS clusters to create"
  type = map(object({
    name               = string
    container_insights = optional(string, "enabled")
    tags               = optional(map(string), {})
  }))
}


variable "task_overrides" {
  description = "Optional overrides for CPU and memory per task family"
  type = map(object({
    cpu    = string
    memory = string
  }))
  default = {}
}


variable "ecs_security_groups" {
  type        = list(string)
  description = "List of ECS security group IDs"
  default     = []
}



variable "log_retention" {
  description = "Retention period (days) for CloudWatch log groups"
  type        = number
  default     = 30
}

variable "tags" {
  description = "Tags applied to ECS log groups"
  type        = map(string)
  default     = {}
}
variable "enable_execute_command" {
  description = "Enable ECS Exec for the service"
  type        = bool
  default     = false
}


variable "schedules" {
  description = "Map of EventBridge schedules to create"

  type = map(object({
    description                        = string
    schedule_expression                = string
    schedule_expression_timezone       = string
    target_arn                         = string
    role_arn                           = string
    input                              = optional(string, "{}")
    state                              = optional(string, "ENABLED")
    flexible_time_window_mode          = optional(string, "OFF")
    retry_maximum_event_age_in_seconds = optional(number, 86400)
    retry_maximum_retry_attempts       = optional(number, 0)

    ecs_target = optional(object({
      task_definition_key          = optional(string)      # reference a local ECS taskdef
      task_definition_arn          = optional(string)      # static ARN fallback
      resolved_task_definition_arn = optional(string)      # injected from root
      use_dynamic_task_def         = optional(bool, false) # <— NEW FLAG

      launch_type      = string
      platform_version = optional(string, "LATEST")

      network_configuration = object({
        subnets          = list(string)
        security_groups  = list(string)
        assign_public_ip = bool
      })
    }))
  }))
  default = {}
}




# --- Logs ---
variable "ecs_log_services" {
  description = "List of ECS service names that need CloudWatch log groups"
  type        = list(string)
}

# --- ECS services (task defs + services) ---
variable "ecs_services" {
  description = "Map of ECS services and their task definitions"
  type = map(object({
    desired_count          = number
    task_definition_key    = string
    enable_execute_command = bool
    launch_type            = string
    subnets                = list(string)
    security_groups        = list(string)
    assign_public_ip       = bool
    cpu                    = string
    memory                 = string
    volumes = optional(list(object({
      name            = string
      file_system_id  = string
      access_point_id = string
    })), [])

    # New optional load balancer block
    load_balancer = optional(list(object({
      target_group_arn = string
      container_name   = string
      container_port   = number
    })), [])
  }))
  default = {
    "headend-gateway" = {
      desired_count          = 1
      task_definition_key    = "headend-gateway"
      enable_execute_command = false
      launch_type            = "FARGATE"
      subnets                = []
      security_groups        = []
      assign_public_ip       = false
      cpu                    = "512"
      memory                 = "1024"
      volumes                = []
      load_balancer          = []
    }
    "pki-api" = {
      desired_count          = 1
      task_definition_key    = "pki-api"
      enable_execute_command = false
      launch_type            = "FARGATE"
      subnets                = []
      security_groups        = []
      assign_public_ip       = false
      cpu                    = "512"
      memory                 = "1024"
      volumes                = []
      load_balancer          = []
    }
    "stepca" = {
      desired_count          = 1
      task_definition_key    = "stepca"
      enable_execute_command = false
      launch_type            = "FARGATE"
      subnets                = []
      security_groups        = []
      assign_public_ip       = false
      cpu                    = "512"
      memory                 = "1024"
      volumes                = []
      load_balancer          = []
    }
  }
}

variable "ecs_task_images" {
  description = "Map of ECS task image URIs keyed by task name (e.g. headend-gateway, pki-api, stepca)"
  type        = map(string)
}




## AustoScaling Variables

variable "resource_id" {
  type        = string
  description = "Resource ID in the form service/clusterName/serviceName"
  default     = ""
}

variable "scalable_dimension" {
  type        = string
  description = "Scalable dimension (ecs:service:DesiredCount)"
  default     = "ecs:service:DesiredCount"
}

variable "min_capacity" {
  type        = number
  description = "Minimum number of tasks"
}

variable "max_capacity" {
  type        = number
  description = "Maximum number of tasks"
}

variable "cpu_target_value" {
  type        = number
  description = "Target CPU utilization %"
}

variable "memory_target_value" {
  type        = number
  description = "Target Memory utilization %"
}

variable "scale_in_cooldown" {
  type        = number
  description = "Cooldown time (seconds) after scale in"
}

variable "scale_out_cooldown" {
  type        = number
  description = "Cooldown time (seconds) after scale out"
}


### Elastic LoadBalancer Variables

variable "lb_name" {
  description = "Base name for the load balancer"
  type        = string
  default     = "unused"
}

variable "lb_internal" {
  description = "Whether the LB is internal (true) or internet-facing (false)"
  type        = bool
  default     = true
}

variable "lb_subnets" {
  description = "List of subnets for the LB"
  type        = list(string)
  default     = []
}

variable "lb_vpc_id" {
  description = "VPC ID for the LB and target group"
  type        = string
  default     = ""
}

variable "lb_listener_port" {
  description = "Listener port for the LB"
  type        = number
  default     = 443
}

variable "lb_target_port" {
  description = "Target group port (container port)"
  type        = number
  default     = 3000
}

variable "lb_health_check_protocol" {
  description = "Protocol for health check (HTTP or TCP)"
  type        = string
  default     = "TCP"
}

# variable "lb_health_check_port" {
#   description = "Port for health check (usually same as target port)"
#   type        = number
#   default     = 8883
# }
