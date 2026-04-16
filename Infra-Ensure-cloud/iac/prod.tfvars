# Global
region           = "us-east-1"
project          = "ensure-cloud"
environment      = "prod"
resource_version = "v1"
owner            = "sentrics"

# StepCA EFS (existing)
stepca_efs_file_system_id  = "fs-06733c0aa322d38d1"
stepca_efs_access_point_id = "fsap-0ee90d5d2e887b1d3"

# Headend API Lambda
headend_api_systems_api_base_url  = "https://apps.ensurelink.net"
headend_api_events_limit_default  = 100
headend_api_events_limit_max      = 1000
headend_api_allow_unauthenticated = false
headend_api_rust_log              = "info"

# Core Change Publisher Lambda
core_change_publisher_systems_api_base_url = "https://apps.ensurelink.net"
core_change_publisher_rust_log             = "info"

# Lambda artifacts (prod uses fixed keys)
lambda_headend_api_s3_key           = "lambda-artifacts/headend-api/headend-api-86ebf6c66879.zip"
lambda_core_change_publisher_s3_key = "lambda-artifacts/core-change-publisher/core-change-publisher-86ebf6c66879.zip"

# ECS Cluster
cluster_map = {
  main = {
    name               = "ensure-cloud-prod"
    container_insights = "enabled"
    tags               = {}
  }
}

# ECS Service Autoscaling
min_capacity        = 1
max_capacity        = 2
cpu_target_value    = 70
memory_target_value = 70
scale_in_cooldown   = 60
scale_out_cooldown  = 60

# Shared Tags
tags = {
  Project     = "ensure-cloud"
  Environment = "prod"
  Owner       = "sentrics"
}

# ECS Logs
ecs_log_services = [
  "headend-gateway",
  "pki-api",
  "stepca"
]

# ECS task images (prod uses fixed tags)
ecs_task_images = {
  headend-gateway = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-headend-gateway:86ebf6c66879"
  pki-api         = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-pki-api:86ebf6c66879"
  stepca          = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-stepca:86ebf6c66879"
}

# ALB (internal)
alb_certificate_arn    = "arn:aws:acm:us-east-1:892234674906:certificate/d3b767d7-7b99-4cb3-b30d-d0662bd9dba2"
alb_trust_store_bucket = "sentrics-ensure-lambda-artifacts-truststore"
alb_trust_store_key    = "trust-store/stepca/ca-bundle.pem"
alb_ingress_cidrs      = ["10.11.0.0/16"]

# ECS Tasks (direct access)
ecs_tasks_ingress_cidrs = ["10.11.0.0/16"]

# API Gateway mTLS
apigw_custom_domain_name = "prod.headend-gateway.ensurelink.net"
apigw_certificate_arn    = "arn:aws:acm:us-east-1:892234674906:certificate/d3b767d7-7b99-4cb3-b30d-d0662bd9dba2"
apigw_trust_store_bucket = "sentrics-ensure-lambda-artifacts-truststore"
apigw_trust_store_key    = "trust-store/stepca/ca-bundle.pem"

