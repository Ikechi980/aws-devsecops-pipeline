environment = "dev"
aws_region  = "us-east-1"

project_name = "sentrics-core"

vpc_name = "Ensure-VPC-Production"

private_subnet_tag_key   = "Tier"
private_subnet_tag_value = "private"


resources_api_lambda_name = "sentrics-core-resources-api"
migrate_lambda_name       = "sentrics-core-db-migration"
lambda_exec_role_name     = "resources-api-lambda-exec"
lambda_sns_policy_name    = "resources-api-sns-publish"
lambda_sg_name            = "resources-api-lambda-sg"
rds_sg_name               = "sentrics-core-db-sg"
database_identifier       = "sentrics-core-db"
db_subnet_group_name      = "sentrics-core-db-subnets"
db_parameter_group_name   = "sentrics-core-db-pg"

api_lambda_s3_key     = "lambda-artifacts/resources-api/resources-api-<RELEASE_SHA>.zip"
migrate_lambda_s3_key = "lambda-artifacts/migrate/migrate-<RELEASE_SHA>.zip"

lambda_s3_bucket = "sentrics-ensure-lambda-artifacts-truststore"



lambda_timeout_seconds = 30
lambda_memory_mb       = 512

database_name     = "resources_db"
database_username = "resources_api_user"

database_instance_class        = "db.t4g.micro"
database_allocated_storage_gb  = 20
database_multi_az              = false
database_backup_retention_days = 3
database_deletion_protection   = false
database_apply_immediately     = true

enable_jwt_auth = true

azure_ad_jwt_issuer   = "https://login.microsoftonline.com/0dbee242-38bd-4e84-b452-b6846e64dc88/v2.0"
azure_ad_jwt_audience = "41615258-2dbc-4c08-9625-dc7c202429fa"

api_name                           = "sentrics-core-resources-api-public"
api_iam_name                       = "sentrics-core-resources-api-internal"
resources_change_events_topic_name = "sentrics-core-resources-change-events"
yardi_sync_failures_topic_name     = "sentrics-core-yardi-sync-failures"

tags = {
  Environment = "dev"
  Owner       = "sentrics"
  Service     = "sentrics-core-api"
}

change_logger_queue_name = "sentrics-core-resources-change-logger-queue"
change_logger_dlq_name   = "sentrics-core-resources-change-logger-dlq"
change_log_table_name    = "sentrics-core-resources-change-log"

yardi_sync_queue_name = "sentrics-core-yardi-sync-queue"
yardi_sync_dlq_name   = "sentrics-core-yardi-sync-dlq"

change_logger_lambda_name      = "sentrics-core-resources-change-logger"
change_logger_lambda_s3_bucket = "sentrics-ensure-lambda-artifacts-truststore"
change_logger_lambda_s3_key    = "lambda-artifacts/resources-change-logger/resources-change-logger-<RELEASE_SHA>.zip"
change_logger_iam_policy_name  = "resources-change-logger-access"


# SNS 


# ECS Cluster

cluster_map = {
  main = {
    name               = "sentrics-core-cluster"
    container_insights = "enabled"
    tags = {
      Owner       = "Ensure"
      Environment = "dev"
      Project     = "sentrics-core"
    }
  }
}

subnets             = ["subnet-03f54fb25d0be3488"]
ecs_security_groups = ["sg-0698efad48a5d0596"]

ecs_task_definition_family         = "yardi-sync"
ecs_task_definition_file           = "yardi-sync-taskdef.json.template"
ecs_container_image                = "892234674906.dkr.ecr.us-east-1.amazonaws.com/sentrics-core-yardi-sync-repo:<RELEASE_SHA>"
ecs_task_cpu                       = "512"
ecs_task_memory                    = "1024"
ecs_service_desired_count          = 1
ecs_service_enable_execute_command = true
ecs_service_assign_public_ip       = true

ecs_task_role_arn = "arn:aws:iam::892234674906:role/ecs-prod-ecs-execution-role"

# ECS Logs
ecs_services  = ["yardi-sync"]
log_retention = 30

# CodePipeline values
pipeline_name                    = "Dev-Sentrics-Core-Master-Pipeline"
artifact_bucket_name             = "sentrics-ensure-terraform-state-codepipeline-cache"
github_owner                     = "SilversphereInc"
github_repo                      = "sentrics-core"
github_branch                    = "development"
infra_github_owner               = "SilversphereInc"
infra_github_repo                = "infra-sentrics-core"
infra_github_branch              = "development"
infra_source_detect_changes      = false
codepipeline_role_arn            = "arn:aws:iam::892234674906:role/service-role/AWSCodePipelineServiceRole-us-east-1-Dev-Sentrics-Master-Orches"
codestar_connection_arn          = "arn:aws:codeconnections:us-east-1:892234674906:connection/ad45d8bb-a719-485b-8b0e-d51fe798dabb"
yardi_image_build_project        = "Sentrics-core-yardi-build-image-pipeline-v1"
lambda_zip_build_project         = "Sentrics-core-lambdas-zip-compile-Pipeline"
infra_build_project              = "Dev-Sentrics-core-infra-pipeline"
yardi_image_repo_name            = "sentrics-core-yardi-sync-repo"
enable_infra_manual_approval     = false
manual_approval_notification_arn = ""
security_scan_project            = "sentrics-core-security-scan-pipeline-v1"
enable_build_stage               = true
enable_security_stage            = true
