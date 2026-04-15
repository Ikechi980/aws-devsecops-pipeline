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

# CodePipeline values
pipeline_name                       = ""
artifact_bucket_name                = "sentrics-ensure-terraform-state-codepipeline-cache"
github_owner                        = "SilversphereInc"
github_repo                         = "ensure-cloud"
github_branch                       = "development"
infra_github_owner                  = "SilversphereInc"
infra_github_repo                   = "infra-ensure-cloud"
infra_github_branch                 = "development"
codepipeline_role_arn               = "arn:aws:iam::892234674906:role/service-role/AWSCodePipelineServiceRole-us-east-1-Dev-Sentrics-Master-Orches"
codestar_connection_arn             = "arn:aws:codeconnections:us-east-1:892234674906:connection/ad45d8bb-a719-485b-8b0e-d51fe798dabb"
stepca_image_build_project          = "ensure-cloud-stepca-build-image-pipeline-v1"
headend_gateway_image_build_project = "ensure-cloud-headend-gateway-build-image-pipeline-v1"
pki_api_image_build_project         = "ensure-cloud-pki-api-build-image-pipeline-v1"
lambda_zip_build_project            = "Ensure-cloud-lambdas-zip-compile-Pipeline"
infra_build_project                 = "Prod-ensure-cloud-infra-pipeline"
headend_gateway_image_repo_name     = "ensure-cloud-headend-gateway"
pki_api_image_repo_name             = "ensure-cloud-pki-api"
stepca_image_repo_name              = "ensure-cloud-stepca"
enable_infra_manual_approval        = true
manual_approval_notification_arn    = ""
enable_build_stage                  = false
security_scan_project               = "ensure-cloud-security-scan-pipeline-v1"
enable_security_stage               = true
