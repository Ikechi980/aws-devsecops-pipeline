environment = "prod"
aws_region  = "us-east-1"

project_name = "sentrics-core"

vpc_id = "vpc-0d5f95e46a0aa79bb"

private_subnet_ids = [
  "subnet-0ceb4c6f3bf403042", # Ensure-private-sub-1
  "subnet-0d68a4043bdaaf613"  # Ensure-private-sub-2
]

resources_api_lambda_name = "sentrics-core-resources-api"
migrate_lambda_name       = "sentrics-core-db-migration"
lambda_exec_role_name     = "resources-api-lambda-exec"
lambda_sns_policy_name    = "resources-api-sns-publish"
lambda_sg_name            = "resources-api-lambda-sg"
rds_sg_name               = "sentrics-core-db-sg"
database_identifier       = "sentrics-core-db"
db_subnet_group_name      = "sentrics-core-db-subnets"
db_parameter_group_name   = "sentrics-core-db-pg"
lambda_s3_bucket          = "sentrics-ensure-lambda-artifacts-truststore"
api_lambda_s3_key         = "prod/lambda-artifacts/resources-api/resources-api-<RELEASE_SHA>.zip"
migrate_lambda_s3_key     = "prod/lambda-artifacts/migrate/migrate-<RELEASE_SHA>.zip"

lambda_timeout_seconds = 30
lambda_memory_mb       = 1024

database_name     = "resources_db"
database_username = "resources_api_user"
database_password = "REPLACE_WITH_PROD_PASSWORD"

database_instance_class        = "db.t4g.small"
database_allocated_storage_gb  = 50
database_multi_az              = true
database_backup_retention_days = 7
database_deletion_protection   = true
database_apply_immediately     = false

enable_jwt_auth = true

azure_ad_jwt_issuer   = "https://login.microsoftonline.com/<TENANT_ID>/v2.0"
azure_ad_jwt_audience = "<AZURE_AD_APP_CLIENT_ID>"

api_name                           = "sentrics-core-resources-api-public"
api_iam_name                       = "sentrics-core-resources-api-internal"
resources_change_events_topic_name = "sentrics-core-resources-change-events"

tags = {
  Environment = "prod"
  Owner       = "sentrics"
  Service     = "sentrics-core-api"
  Criticality = "high"
}

change_logger_queue_name = "sentrics-core-resources-change-logger-queue"
change_logger_dlq_name   = "sentrics-core-resources-change-logger-dlq"
change_log_table_name    = "sentrics-core-resources-change-log"

change_logger_lambda_name      = "sentrics-core-resources-change-logger"
change_logger_lambda_s3_bucket = "sentrics-ensure-lambda-artifacts-truststore"
change_logger_lambda_s3_key    = "prod/lambda-artifacts/resources-change-logger/resources-change-logger-<RELEASE_SHA>.zip"
change_logger_iam_policy_name  = "resources-change-logger-access"

# ECS image
ecs_task_definition_file = "yardi-sync-taskdef.json.template"
ecs_container_image      = "892234674906.dkr.ecr.us-east-1.amazonaws.com/prod-sentrics-core-yardi-sync-repo:<RELEASE_SHA>"
