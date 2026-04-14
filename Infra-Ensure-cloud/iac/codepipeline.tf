data "aws_s3_bucket" "artifact_store" {
  bucket = var.artifact_bucket_name
}

resource "aws_codepipeline" "master" {
  name     = local.pipeline_name_effective
  role_arn = var.codepipeline_role_arn

  artifact_store {
    location = data.aws_s3_bucket.artifact_store.bucket
    type     = "S3"
  }

  stage {
    name = "Source"

    action {
      name             = "AppSource"
      namespace        = "AppSourceVars"
      category         = "Source"
      owner            = "AWS"
      provider         = "CodeStarSourceConnection"
      version          = "1"
      output_artifacts = ["AppSource"]
      configuration = {
        ConnectionArn    = var.codestar_connection_arn
        FullRepositoryId = "${var.github_owner}/${var.github_repo}"
        BranchName       = var.github_branch
      }
    }

    action {
      name             = "InfraSource"
      category         = "Source"
      owner            = "AWS"
      provider         = "CodeStarSourceConnection"
      version          = "1"
      output_artifacts = ["InfraSource"]
      configuration = {
        ConnectionArn    = var.codestar_connection_arn
        FullRepositoryId = "${var.infra_github_owner}/${var.infra_github_repo}"
        BranchName       = var.infra_github_branch
        DetectChanges    = "false"
      }
    }
  }

  dynamic "stage" {
    for_each = var.enable_build_stage ? [1] : []
    content {
      name = "BuildArtifacts"

      action {
        name             = "BuildStepcaImage"
        category         = "Build"
        owner            = "AWS"
        provider         = "CodeBuild"
        version          = "1"
        input_artifacts  = ["AppSource"]
        run_order        = 1
        configuration = {
          ProjectName          = var.stepca_image_build_project
          EnvironmentVariables = local.build_action_env_vars
        }
      }

      action {
        name             = "BuildHeadendGatewayImage"
        category         = "Build"
        owner            = "AWS"
        provider         = "CodeBuild"
        version          = "1"
        input_artifacts  = ["AppSource"]
        run_order        = 1
        configuration = {
          ProjectName          = var.headend_gateway_image_build_project
          EnvironmentVariables = local.build_action_env_vars
        }
      }

      action {
        name             = "BuildPkiApiImage"
        category         = "Build"
        owner            = "AWS"
        provider         = "CodeBuild"
        version          = "1"
        input_artifacts  = ["AppSource"]
        run_order        = 1
        configuration = {
          ProjectName          = var.pki_api_image_build_project
          EnvironmentVariables = local.build_action_env_vars
        }
      }

      action {
        name             = "BuildLambdaZips"
        category         = "Build"
        owner            = "AWS"
        provider         = "CodeBuild"
        version          = "1"
        input_artifacts  = ["AppSource"]
        run_order        = 1
        configuration = {
          ProjectName          = var.lambda_zip_build_project
          EnvironmentVariables = local.build_action_env_vars
        }
      }
    }
  }

  dynamic "stage" {
    for_each = var.enable_build_stage && var.enable_security_stage ? [1] : []
    content {
      name = "SecurityGate"

      action {
        name            = "SecurityScanAndPublish"
        category        = "Build"
        owner           = "AWS"
        provider        = "CodeBuild"
        version         = "1"
        input_artifacts = ["AppSource"]
        run_order = 1
        configuration = {
          ProjectName          = var.security_scan_project
          PrimarySource        = "AppSource"
          EnvironmentVariables = local.security_action_env_vars
        }
      }
    }
  }

  dynamic "stage" {
    for_each = var.enable_infra_manual_approval ? [1] : []
    content {
      name = "ManualApproval"

      action {
        name      = "ApproveDeploy"
        category  = "Approval"
        owner     = "AWS"
        provider  = "Manual"
        version   = "1"
        run_order = 1
        configuration = merge(
          {
            CustomData = "Approve deployment to ${var.environment}"
          },
          var.manual_approval_notification_arn != "" ? {
            NotificationArn = var.manual_approval_notification_arn
          } : {}
        )
      }
    }
  }

  stage {
    name = "DeployInfrastructure"

    action {
      name            = "DeployInfra"
      category        = "Build"
      owner           = "AWS"
      provider        = "CodeBuild"
      version         = "1"
      input_artifacts = ["InfraSource"]
      run_order       = 1
      configuration = {
        ProjectName          = var.infra_build_project
        PrimarySource        = "InfraSource"
        EnvironmentVariables = local.deploy_action_env_vars
      }
    }
  }
}

variable "pipeline_name" {
  description = "Optional explicit CodePipeline name. When empty, a name is derived from project and environment."
  type        = string
  default     = ""
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

variable "stepca_image_build_project" {
  type = string
}

variable "headend_gateway_image_build_project" {
  type = string
}

variable "pki_api_image_build_project" {
  type = string
}

variable "lambda_zip_build_project" {
  type = string
}

variable "security_scan_project" {
  description = "CodeBuild project that scans build artifacts and publishes only on pass."
  type        = string
}

variable "infra_build_project" {
  type = string
}

variable "headend_gateway_image_repo_name" {
  description = "ECR repository name for headend-gateway image (without registry URL)."
  type        = string
}

variable "pki_api_image_repo_name" {
  description = "ECR repository name for pki-api image (without registry URL)."
  type        = string
}

variable "stepca_image_repo_name" {
  description = "ECR repository name for stepca image (without registry URL)."
  type        = string
}

variable "enable_infra_manual_approval" {
  description = "Require a manual approval action before DeployInfrastructure stage."
  type        = bool
  default     = false
}

variable "manual_approval_notification_arn" {
  description = "Optional SNS topic ARN for manual approval notifications."
  type        = string
  default     = ""
}

variable "enable_build_stage" {
  description = "Whether to include the BuildArtifacts stage in this pipeline."
  type        = bool
  default     = true
}

variable "enable_security_stage" {
  description = "Whether to include the SecurityGate stage in this pipeline."
  type        = bool
  default     = true
}
