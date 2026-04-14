# Global
region         = "us-east-1"
project        = "ensure-cloud"
environment    = "dev"
resource_version = "v1"
owner          = "sentrics"

# StepCA EFS (existing)
stepca_efs_file_system_id            = "fs-06733c0aa322d38d1"
stepca_efs_access_point_id           = "fsap-0ee90d5d2e887b1d3"

# Networking
lb_subnets = ["subnet-03f54fb25d0be3488", "subnet-0d4c29bf28473c856"]

# Headend API Lambda
headend_api_systems_api_base_url        = "https://dev.ems.ensurelink.net"
headend_api_events_limit_default        = 100
headend_api_events_limit_max            = 1000
headend_api_rust_log                    = "info"

# Core Change Publisher Lambda
core_change_publisher_systems_api_base_url = "https://dev.ems.ensurelink.net"
core_change_publisher_rust_log              = "info"

# Lambda artifacts
lambda_headend_api_s3_key           = "lambda-artifacts/headend-api/headend-api-<RELEASE_SHA>.zip"
lambda_core_change_publisher_s3_key = "lambda-artifacts/core-change-publisher/core-change-publisher-<RELEASE_SHA>.zip"

# ECS Cluster
cluster_map = {
  main = {
    name               = "ensure-cloud-dev"
    container_insights = "enabled"
    tags               = {}
  }
}

# ECS Service Autoscaling
min_capacity       = 1
max_capacity       = 2
cpu_target_value   = 70
memory_target_value = 70
scale_in_cooldown  = 60
scale_out_cooldown = 60

# Shared Tags
tags = {
  Project     = "ensure-cloud"
  Environment = "dev"
  Owner       = "sentrics"
}

# ECS Logs
ecs_log_services = [
  "headend-gateway",
  "pki-api",
  "stepca"
]

# ECS task images
ecs_task_images = {
  headend-gateway = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-headend-gateway:<RELEASE_SHA>"
  pki-api         = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-pki-api:<RELEASE_SHA>"
  stepca          = "892234674906.dkr.ecr.us-east-1.amazonaws.com/ensure-cloud-stepca:<RELEASE_SHA>"
}

# ALB
alb_certificate_arn   = "arn:aws:acm:us-east-1:892234674906:certificate/55ee4701-8efb-48e7-b35c-3d83c4f92575"
alb_trust_store_bucket = "sentrics-ensure-lambda-artifacts-truststore"
alb_trust_store_key    = "trust-store/stepca/ca-bundle.pem"
alb_ingress_cidrs      = ["0.0.0.0/0"]
alb_custom_domain_name = "dev.headend-gateway.ensurelink.net"

# ECS Tasks (direct access)
ecs_tasks_ingress_cidrs = ["10.11.0.0/16"]
wg_ingress_cidrs        = ["172.16.128.0/20", "192.168.142.0/24"]

# API Gateway mTLS
apigw_custom_domain_name = "dev.headend-api.ensurelink.net"
apigw_certificate_arn    = "arn:aws:acm:us-east-1:892234674906:certificate/55ee4701-8efb-48e7-b35c-3d83c4f92575"
apigw_trust_store_bucket = "sentrics-ensure-lambda-artifacts-truststore"
apigw_trust_store_key    = "trust-store/stepca/ca-bundle.pem"


# CodePipeline values
pipeline_name           = ""
artifact_bucket_name    = "sentrics-ensure-terraform-state-codepipeline-cache"
github_owner            = "SilversphereInc"
github_repo             = "ensure-cloud"
github_branch           = "development"
infra_github_owner      = "SilversphereInc"
infra_github_repo       = "infra-ensure-cloud"
infra_github_branch     = "development"
codepipeline_role_arn   = "arn:aws:iam::892234674906:role/service-role/AWSCodePipelineServiceRole-us-east-1-Dev-Sentrics-Master-Orches"
codestar_connection_arn = "arn:aws:codeconnections:us-east-1:892234674906:connection/ad45d8bb-a719-485b-8b0e-d51fe798dabb"
stepca_image_build_project          = "ensure-cloud-stepca-build-image-pipeline-v1"
headend_gateway_image_build_project = "ensure-cloud-headend-gateway-build-image-pipeline-v1"
pki_api_image_build_project         = "ensure-cloud-pki-api-build-image-pipeline-v1"
lambda_zip_build_project            = "Ensure-cloud-lambdas-zip-compile-Pipeline"
infra_build_project                 = "Dev-ensure-cloud-infra-pipeline"
headend_gateway_image_repo_name     = "ensure-cloud-headend-gateway"
pki_api_image_repo_name             = "ensure-cloud-pki-api"
stepca_image_repo_name              = "ensure-cloud-stepca"
enable_infra_manual_approval        = false
manual_approval_notification_arn    = ""
enable_build_stage                  = true
security_scan_project             = "ensure-cloud-security-scan-pipeline-v1"
enable_security_stage             = true
