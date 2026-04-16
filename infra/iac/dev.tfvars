# =============================================================================
# Shared
# =============================================================================
region      = "us-east-1"
environment = "dev"

tags = {
  Environment = "dev"
  Owner       = "sentrics"
  ManagedBy   = "terraform"
}

log_retention        = 30
lambda_s3_bucket     = "sentrics-ensure-lambda-artifacts-truststore"
ecs_task_role_arn    = "arn:aws:iam::892234674906:role/ecs-prod-ecs-execution-role"

# =============================================================================
# Sentrics-Core
# =============================================================================
project_name = "sentrics-core"

vpc_name                 = "Ensure-VPC-Production"
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

lambda_timeout_seconds = 30
lambda_memory_mb       = 512

database_name                  = "resources_db"
database_username              = "resources_api_user"
database_instance_class        = "db.t4g.micro"
database_allocated_storage_gb  = 20
database_multi_az              = false
database_backup_retention_days = 3
database_deletion_protection   = false
database_apply_immediately     = true

enable_jwt_auth       = true
azure_ad_jwt_issuer   = "https://login.microsoftonline.com/0dbee242-38bd-4e84-b452-b6846e64dc88/v2.0"
azure_ad_jwt_audience = "41615258-2dbc-4c08-9625-dc7c202429fa"

api_name                           = "sentrics-core-resources-api-public"
api_iam_name                       = "sentrics-core-resources-api-internal"
resources_change_events_topic_name = "sentrics-core-resources-change-events"
yardi_sync_failures_topic_name     = "sentrics-core-yardi-sync-failures"

change_logger_queue_name = "sentrics-core-resources-change-logger-queue"
change_logger_dlq_name   = "sentrics-core-resources-change-logger-dlq"
change_log_table_name    = "sentrics-core-resources-change-log"
yardi_sync_queue_name    = "sentrics-core-yardi-sync-queue"
yardi_sync_dlq_name      = "sentrics-core-yardi-sync-dlq"

change_logger_lambda_name      = "sentrics-core-resources-change-logger"
change_logger_lambda_s3_bucket = "sentrics-ensure-lambda-artifacts-truststore"
change_logger_lambda_s3_key    = "lambda-artifacts/resources-change-logger/resources-change-logger-<RELEASE_SHA>.zip"
change_logger_iam_policy_name  = "resources-change-logger-access"

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

ecs_services  = ["yardi-sync"]


# =============================================================================
# Ensure-Cloud
# =============================================================================
project          = "ensure-cloud"
resource_version = "v1"
owner            = "sentrics"

stepca_efs_file_system_id  = "fs-06733c0aa322d38d1"
stepca_efs_access_point_id = "fsap-0ee90d5d2e887b1d3"

lb_subnets = ["subnet-03f54fb25d0be3488", "subnet-0d4c29bf28473c856"]

headend_api_systems_api_base_url           = "https://dev.ems.ensurelink.net"
headend_api_events_limit_default           = 100
headend_api_events_limit_max               = 1000
headend_api_rust_log                       = "info"
core_change_publisher_systems_api_base_url = "https://dev.ems.ensurelink.net"
core_change_publisher_rust_log             = "info"

lambda_headend_api_s3_key           = "lambda-artifacts/headend-api/headend-api-<RELEASE_SHA>.zip"
lambda_core_change_publisher_s3_key = "lambda-artifacts/core-change-publisher/core-change-publisher-<RELEASE_SHA>.zip"

ec_cluster_map = {
  main = {
    name               = "ensure-cloud-dev"
    container_insights = "enabled"
    tags               = {}
  }
}

min_capacity        = 1
max_capacity        = 2
cpu_target_value    = 70
memory_target_value = 70
scale_in_cooldown   = 60
scale_out_cooldown  = 60

ecs_log_services = ["headend-gateway", "pki-api", "stepca"]

ecs_task_images = {
  headend-gateway = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-headend-gateway:<RELEASE_SHA>"
  pki-api         = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-pki-api:<RELEASE_SHA>"
  stepca          = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-stepca:<RELEASE_SHA>"
}

alb_certificate_arn    = "arn:aws:acm:us-east-1:892234674906:certificate/55ee4701-8efb-48e7-b35c-3d83c4f92575"
alb_trust_store_bucket = "sentrics-ensure-lambda-artifacts-truststore"
alb_trust_store_key    = "trust-store/stepca/ca-bundle.pem"
alb_ingress_cidrs      = ["0.0.0.0/0"]
alb_custom_domain_name = "dev.headend-gateway.ensurelink.net"

ecs_tasks_ingress_cidrs = ["10.11.0.0/16"]
wg_ingress_cidrs        = ["172.16.128.0/20", "192.168.142.0/24"]

apigw_custom_domain_name = "dev.headend-api.ensurelink.net"
apigw_certificate_arn    = "arn:aws:acm:us-east-1:892234674906:certificate/55ee4701-8efb-48e7-b35c-3d83c4f92575"
apigw_trust_store_bucket = "sentrics-ensure-lambda-artifacts-truststore"
apigw_trust_store_key    = "trust-store/stepca/ca-bundle.pem"
