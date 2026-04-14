// variables.tf

variable "project_name" {
  type    = string
  default = "sentrics-core"
}

variable "environment" {
  type = string
}

variable "aws_region" {
  type    = string
  default = "us-east-1"
}

variable "tags" {
  type    = map(string)
  default = {}
}

variable "vpc_name" {
  type = string
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

variable "lambda_s3_bucket" {
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

variable "database_engine_major" {
  type    = string
  default = "16"
}

variable "database_engine_version" {
  type        = string
  default     = "16"
  description = "Optional. If null, AWS default engine version is used for the major version family."
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
  type        = string
  default     = ""
  description = "Example: https://login.microsoftonline.com/<tenant_id>/v2.0"
}

variable "azure_ad_jwt_audience" {
  type        = string
  default     = ""
  description = "Typically your Azure AD app client id, or an app id URI depending on your setup."
}

variable "resources_change_events_topic_name" {
  type = string
}

variable "yardi_sync_failures_topic_name" {
  type = string
}

variable "migrate_lambda_name" {
  type = string
  description = "The name for the migration lambda"
  
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

# ECS-Cluster

variable "cluster_map" {
  description = "Map of ECS clusters to create"
  type = map(object({
    name               = string
    container_insights = optional(string, "enabled")
    tags               = optional(map(string), {})
  }))
}


 

variable "ecs_task_role_arn" {
  description = "IAM role ARN to use for ECS task execution and task role"
  type        = string
}

variable "ecs_task_definition_family" {
  description = "ECS task definition family"
  type        = string
}

variable "ecs_task_definition_file" {
  description = "Path to the ECS task definition JSON (relative to repo root)"
  type        = string
}

variable "ecs_container_image" {
  description = "Full ECS container image URI including tag"
  type        = string
}

variable "ecs_task_cpu" {
  description = "CPU units for the ECS task"
  type        = string
  default     = "512"
}

variable "ecs_task_memory" {
  description = "Memory (MB) for the ECS task"
  type        = string
  default     = "1024"
}

variable "ecs_service_desired_count" {
  description = "Desired count for the ECS service"
  type        = number
  default     = 1
}

variable "ecs_service_enable_execute_command" {
  description = "Enable ECS Exec for the service"
  type        = bool
  default     = false
}

variable "ecs_service_assign_public_ip" {
  description = "Assign a public IP to the ECS task ENI"
  type        = bool
  default     = true
}

variable "subnets" {
  description = "Subnets for the ECS service"
  type        = list(string)
}

variable "ecs_security_groups" {
  description = "Security groups for the ECS service"
  type        = list(string)
}

variable "ecs_services" {
  description = "List of ECS service names for CloudWatch log groups"
  type        = list(string)
}

variable "log_retention" {
  description = "Number of days to retain ECS logs in CloudWatch"
  type        = number
  default     = 30
}


# CodePipeline Variables.


variable "pipeline_name" {
  type    = string
  default = "Dev-Sentrics-Core-Master-Pipeline"
}

variable "codepipeline_role_arn" {
  type = string
}

variable "artifact_bucket_name" {
  type    = string
  default = "sentrics-ensure-terraform-state-codepipeline-cache"
}

variable "codestar_connection_arn" {
  type = string
}

variable "github_owner" {
  type = string
}

variable "github_repo" {
  type = string
}

variable "github_branch" {
  type    = string
  default = "development"
}

variable "infra_github_owner" {
  type = string
}

variable "infra_github_repo" {
  type = string
}

variable "infra_github_branch" {
  type    = string
  default = "development"
}

variable "yardi_image_build_project" {
  type = string
}

variable "lambda_zip_build_project" {
  type = string
}

variable "infra_build_project" {
  type = string
}

variable "yardi_image_repo_name" {
  description = "ECR repository name for yardi image (without registry URL)."
  type        = string
}

variable "infra_source_detect_changes" {
  description = "Whether infra source changes should auto-trigger the pipeline"
  type        = bool
  default     = false
}

variable "enable_infra_manual_approval" {
  description = "Require a manual approval action before DeployInfrastructure stage"
  type        = bool
  default     = false
}

variable "manual_approval_notification_arn" {
  description = "Optional SNS topic ARN for manual approval notifications"
  type        = string
  default     = ""
}

variable "security_scan_project" {
  description = "CodeBuild project that scans build artifacts and publishes only on pass"
  type        = string
}

variable "enable_build_stage" {
  description = "Whether to include the BuildArtifacts stage in this pipeline"
  type        = bool
  default     = true
}

variable "enable_security_stage" {
  description = "Whether to include the SecurityGate stage in this pipeline"
  type        = bool
  default     = true
}
