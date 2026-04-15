data "aws_s3_bucket" "artifact_store" {
  bucket = var.artifact_bucket_name
}

resource "aws_codepipeline" "master" {
  name     = var.pipeline_name
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
        DetectChanges    = tostring(var.infra_source_detect_changes)
      }
    }
  }

  dynamic "stage" {
    for_each = var.enable_build_stage ? [1] : []
    content {
      name = "BuildArtifacts"

      action {
        name            = "BuildYardiImage"
        category        = "Build"
        owner           = "AWS"
        provider        = "CodeBuild"
        version         = "1"
        input_artifacts = ["AppSource"]
        run_order       = 1
        configuration = {
          ProjectName          = var.yardi_image_build_project
          EnvironmentVariables = local.build_action_env_vars
        }
      }

      action {
        name            = "BuildLambdaZips"
        category        = "Build"
        owner           = "AWS"
        provider        = "CodeBuild"
        version         = "1"
        input_artifacts = ["AppSource"]
        run_order       = 1
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
        run_order       = 1
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
      name = "ApprovalBeforeDeploy"

      action {
        name      = "ManualApproveInfra"
        category  = "Approval"
        owner     = "AWS"
        provider  = "Manual"
        version   = "1"
        run_order = 1
        configuration = merge(
          {
            CustomData = "Approve infrastructure deployment for ${var.environment}."
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
