# =============================================================================
# Shared variables — used by both stacks
# =============================================================================

variable "region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name (dev, prod)"
  type        = string
}

variable "tags" {
  description = "Common tags applied to all resources"
  type        = map(string)
  default     = {}
}

variable "log_retention" {
  description = "CloudWatch log group retention in days"
  type        = number
  default     = 30
}

variable "lambda_s3_bucket" {
  description = "S3 bucket holding Lambda zip artifacts"
  type        = string
  default     = "sentrics-ensure-lambda-artifacts-truststore"
}

variable "ecs_task_role_arn" {
  description = "IAM role ARN used for ECS task execution and task role"
  type        = string
  default     = "arn:aws:iam::892234674906:role/ecs-prod-ecs-execution-role"
}

# =============================================================================
# Sentrics-Core variables
# =============================================================================

variable "project_name" {
  description = "Sentrics-core project name"
  type        = string
  default     = "sentrics-core"
}

variable "vpc_name" {
  description = "Name tag of the VPC (used by data source lookup)"
  type        = string
}

variable "private_subnet_tag_key" {
  type = string
}

variable "private_subnet_tag_value" {
  type = string
}

variable "resources_api_lambda_name" {
  type    = string
  default = "sentrics-core-resources-api"
}

variable "lambda_exec_role_name" {
  type = string
}

variable "lambda_sns_policy_name" {
  type = string
}

variable "api_lambda_s3_key" {
  type = string
}

variable "migrate_lambda_s3_key" {
  type = string
}

variable "lambda_timeout_seconds" {
  type    = number
  default = 30
}

variable "lambda_memory_mb" {
  type    = number
  default = 512
}

variable "database_name" {
  type    = string
  default = "resources_db"
}

variable "database_username" {
  type    = string
  default = "resources_api_user"
}

variable "database_port" {
  type    = number
  default = 5432
}

variable "database_allocated_storage_gb" {
  type    = number
  default = 20
}

variable "database_instance_class" {
  type    = string
  default = "db.t4g.micro"
}

variable "database_multi_az" {
  type    = bool
  default = false
}

variable "database_backup_retention_days" {
  type    = number
  default = 7
}

variable "database_deletion_protection" {
  type    = bool
  default = true
}

variable "database_publicly_accessible" {
  type    = bool
  default = false
}

variable "database_engine_version" {
  type    = string
  default = "16"
}

variable "database_apply_immediately" {
  type    = bool
  default = true
}

variable "api_name" {
  type    = string
  default = "sentrics-core-resources-api-public"
}

variable "api_iam_name" {
  type    = string
  default = "sentrics-core-resources-api-internal"
}

variable "enable_jwt_auth" {
  type    = bool
  default = true
}

variable "azure_ad_jwt_issuer" {
  type    = string
  default = ""
}

variable "azure_ad_jwt_audience" {
  type    = string
  default = ""
}

variable "resources_change_events_topic_name" {
  type = string
}

variable "yardi_sync_failures_topic_name" {
  type = string
}

variable "migrate_lambda_name" {
  type = string
}

variable "lambda_sg_name" {
  type = string
}

variable "rds_sg_name" {
  type = string
}

variable "database_identifier" {
  type = string
}

variable "db_subnet_group_name" {
  type = string
}

variable "db_parameter_group_name" {
  type = string
}

variable "change_logger_queue_name" {
  type = string
}

variable "change_logger_dlq_name" {
  type = string
}

variable "change_logger_queue_visibility_timeout_seconds" {
  type    = number
  default = 90
}

variable "change_logger_max_receive_count" {
  type    = number
  default = 5
}

variable "yardi_sync_queue_name" {
  type = string
}

variable "yardi_sync_dlq_name" {
  type = string
}

variable "change_log_table_name" {
  type = string
}

variable "change_logger_lambda_name" {
  type = string
}

variable "change_logger_lambda_s3_bucket" {
  type = string
}

variable "change_logger_lambda_s3_key" {
  type = string
}

variable "change_logger_lambda_timeout_seconds" {
  type    = number
  default = 60
}

variable "change_logger_lambda_memory_mb" {
  type    = number
  default = 512
}

variable "change_logger_batch_size" {
  type    = number
  default = 10
}

variable "change_logger_rust_log" {
  type    = string
  default = ""
}

variable "change_logger_iam_policy_name" {
  type = string
}

# Sentrics-core ECS
variable "cluster_map" {
  description = "ECS clusters for sentrics-core"
  type = map(object({
    name               = string
    container_insights = optional(string, "enabled")
    tags               = optional(map(string), {})
  }))
}

variable "ecs_task_definition_family" {
  type = string
}

variable "ecs_task_definition_file" {
  type = string
}

variable "ecs_container_image" {
  type = string
}

variable "ecs_task_cpu" {
  type    = string
  default = "512"
}

variable "ecs_task_memory" {
  type    = string
  default = "1024"
}

variable "ecs_service_desired_count" {
  type    = number
  default = 1
}

variable "ecs_service_enable_execute_command" {
  type    = bool
  default = false
}

variable "ecs_service_assign_public_ip" {
  type    = bool
  default = true
}

variable "subnets" {
  description = "Subnets for the sentrics-core ECS service"
  type        = list(string)
}

variable "ecs_security_groups" {
  description = "Security groups for sentrics-core ECS service and SNS endpoint rules"
  type        = list(string)
  default     = []
}

variable "ecs_services" {
  description = "Sentrics-core ECS service names for CloudWatch log groups"
  type        = list(string)
}

# =============================================================================
# Ensure-Cloud variables
# =============================================================================

variable "project" {
  description = "Ensure-cloud project name"
  type        = string
}

variable "resource_version" {
  type = string
}

variable "owner" {
  type = string
}

variable "vpc_id" {
  description = "VPC ID for ensure-cloud resources"
  type        = string
  default     = "vpc-0d5f95e46a0aa79bb"
}

variable "private_subnets" {
  description = "Private subnet IDs for ensure-cloud resources"
  type        = list(string)
  default     = ["subnet-0ceb4c6f3bf403042", "subnet-0d68a4043bdaaf613"]
}

variable "ecs_execution_role_arn" {
  description = "ECS task execution role ARN for ensure-cloud"
  type        = string
  default     = "arn:aws:iam::892234674906:role/ecs-prod-ecs-execution-role"
}

variable "stepca_efs_file_system_id" {
  type = string
}

variable "stepca_efs_access_point_id" {
  type = string
}

variable "lambda_headend_api_s3_key" {
  type = string
}

variable "lambda_core_change_publisher_s3_key" {
  type = string
}

variable "headend_api_systems_api_base_url" {
  type = string
}

variable "headend_api_events_mongo_url_ssm_parameter" {
  type    = string
  default = "/ensure-cloud/headend-api/events-mongo-url"
}

variable "headend_api_events_limit_default" {
  type    = number
  default = 100
}

variable "headend_api_events_limit_max" {
  type    = number
  default = 1000
}

variable "headend_api_allow_unauthenticated" {
  type    = bool
  default = false
}

variable "headend_api_rust_log" {
  type    = string
  default = "info"
}

variable "core_change_publisher_systems_api_base_url" {
  type = string
}

variable "core_change_publisher_aws_endpoint_url" {
  type    = string
  default = ""
}

variable "core_change_publisher_rust_log" {
  type    = string
  default = "info"
}

variable "alb_certificate_arn" {
  type = string
}

variable "alb_trust_store_bucket" {
  type = string
}

variable "alb_trust_store_key" {
  type = string
}

variable "alb_ingress_cidrs" {
  type    = list(string)
  default = []
}

variable "alb_custom_domain_name" {
  type    = string
  default = ""
}

variable "ecs_tasks_ingress_cidrs" {
  type    = list(string)
  default = []
}

variable "wg_ingress_cidrs" {
  type    = list(string)
  default = []
}

variable "apigw_custom_domain_name" {
  type = string
}

variable "apigw_certificate_arn" {
  type = string
}

variable "apigw_trust_store_bucket" {
  type = string
}

variable "apigw_trust_store_key" {
  type = string
}

variable "public_hosted_zone_name" {
  type    = string
  default = "ensurelink.net"
}

# Ensure-cloud ECS
variable "ec_cluster_map" {
  description = "ECS clusters for ensure-cloud"
  type = map(object({
    name               = string
    container_insights = optional(string, "enabled")
    tags               = optional(map(string), {})
  }))
}

variable "task_overrides" {
  description = "Optional CPU/memory overrides per task family"
  type = map(object({
    cpu    = string
    memory = string
  }))
  default = {}
}

variable "ecs_log_services" {
  description = "Ensure-cloud ECS service names for CloudWatch log groups"
  type        = list(string)
}

variable "ecs_task_images" {
  description = "Map of ECS task image URIs for ensure-cloud (headend-gateway, pki-api, stepca)"
  type        = map(string)
}

variable "min_capacity" {
  type = number
}

variable "max_capacity" {
  type = number
}

variable "cpu_target_value" {
  type = number
}

variable "memory_target_value" {
  type = number
}

variable "scale_in_cooldown" {
  type = number
}

variable "scale_out_cooldown" {
  type = number
}

variable "lb_subnets" {
  type    = list(string)
  default = []
}

variable "schedules" {
  description = "Map of EventBridge schedules"
  type        = any
  default     = {}
}
