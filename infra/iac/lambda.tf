# =============================================================================
# Sentrics-Core Lambdas
# =============================================================================

resource "aws_lambda_function" "api" {
  function_name = "${var.resources_api_lambda_name}-${var.environment}"
  role          = aws_iam_role.lambda_exec.arn

  s3_bucket = var.lambda_s3_bucket
  s3_key    = var.api_lambda_s3_key

  runtime = "provided.al2023"
  handler = "bootstrap"

  timeout     = var.lambda_timeout_seconds
  memory_size = var.lambda_memory_mb
  publish     = true

  vpc_config {
    subnet_ids         = local.private_subnet_ids
    security_group_ids = [aws_security_group.lambda.id]
  }

  environment {
    variables = {
      RUST_LOG      = "info"
      SNS_TOPIC_ARN = aws_sns_topic.resources_change_events.arn
      DATABASE_URL  = "postgres://${var.database_username}:${urlencode(random_password.db.result)}@${aws_db_instance.this.address}:${var.database_port}/${var.database_name}"
    }
  }

  tags = local.merged_tags

  depends_on = [
    aws_iam_role_policy_attachment.lambda_basic_logs,
    aws_iam_role_policy_attachment.lambda_vpc_access
  ]
}

resource "aws_lambda_function" "migrate" {
  function_name = "${var.migrate_lambda_name}-${var.environment}"
  role          = aws_iam_role.lambda_exec.arn

  s3_bucket = var.lambda_s3_bucket
  s3_key    = var.migrate_lambda_s3_key

  runtime = "provided.al2023"
  handler = "bootstrap"

  timeout     = var.lambda_timeout_seconds
  memory_size = var.lambda_memory_mb
  publish     = true

  vpc_config {
    subnet_ids         = local.private_subnet_ids
    security_group_ids = [aws_security_group.lambda.id]
  }

  environment {
    variables = {
      RUST_LOG     = "info"
      DATABASE_URL = "postgres://${var.database_username}:${urlencode(random_password.db.result)}@${aws_db_instance.this.address}:${var.database_port}/${var.database_name}"
    }
  }

  tags = local.merged_tags

  depends_on = [
    aws_iam_role_policy_attachment.lambda_basic_logs,
    aws_iam_role_policy_attachment.lambda_vpc_access
  ]
}

resource "aws_lambda_function" "resources_change_logger" {
  function_name = "${var.change_logger_lambda_name}-${var.environment}"
  role          = aws_iam_role.lambda_exec.arn

  s3_bucket = var.change_logger_lambda_s3_bucket
  s3_key    = var.change_logger_lambda_s3_key

  runtime = "provided.al2023"
  handler = "bootstrap"

  timeout     = var.change_logger_lambda_timeout_seconds
  memory_size = var.change_logger_lambda_memory_mb
  publish     = true

  environment {
    variables = {
      CHANGE_LOG_TABLE_NAME = aws_dynamodb_table.resources_change_log.name
      RUST_LOG              = var.change_logger_rust_log
    }
  }

  tags = local.merged_tags

  depends_on = [aws_iam_role_policy_attachment.lambda_basic_logs]
}

resource "aws_lambda_event_source_mapping" "resources_change_logger" {
  event_source_arn        = aws_sqs_queue.resources_change_logger.arn
  function_name           = aws_lambda_function.resources_change_logger.arn
  batch_size              = var.change_logger_batch_size
  enabled                 = true
  function_response_types = ["ReportBatchItemFailures"]
}

# =============================================================================
# Ensure-Cloud Lambdas
# =============================================================================

resource "aws_lambda_function" "headend_api" {
  function_name = "${var.project}-${var.environment}-headend-api"
  role          = aws_iam_role.lambda_headend_api.arn
  handler       = "bootstrap"
  runtime       = "provided.al2023"
  s3_bucket     = var.lambda_s3_bucket
  s3_key        = var.lambda_headend_api_s3_key

  timeout     = 30
  memory_size = 512

  environment {
    variables = merge(
      {
        SYSTEMS_API_BASE_URL           = var.headend_api_systems_api_base_url
        # Cross-stack fix: was data.terraform_remote_state.sentrics_core.outputs.container_api_endpoint
        CORE_RESOURCES_API_BASE_URL    = aws_apigatewayv2_api.iam.api_endpoint
        EVENTS_MONGO_URL_SSM_PARAMETER = var.headend_api_events_mongo_url_ssm_parameter
        EVENTS_LIMIT_DEFAULT           = tostring(var.headend_api_events_limit_default)
        EVENTS_LIMIT_MAX               = tostring(var.headend_api_events_limit_max)
        RUST_LOG                       = var.headend_api_rust_log
      },
      var.headend_api_allow_unauthenticated ? { ALLOW_UNAUTHENTICATED = "1" } : {}
    )
  }

  vpc_config {
    subnet_ids         = var.private_subnets
    security_group_ids = [aws_security_group.ec_lambda.id]
  }

  tags = var.tags
}

resource "aws_lambda_function" "core_change_publisher" {
  function_name = "${var.project}-${var.environment}-core-change-publisher"
  role          = aws_iam_role.lambda_core_change_publisher.arn
  handler       = "bootstrap"
  runtime       = "provided.al2023"
  s3_bucket     = var.lambda_s3_bucket
  s3_key        = var.lambda_core_change_publisher_s3_key

  timeout     = 30
  memory_size = 512

  environment {
    variables = merge(
      {
        SYSTEMS_API_BASE_URL  = var.core_change_publisher_systems_api_base_url
        HEADEND_SNS_TOPIC_ARN = module.headend_messages_sns.topic_arn
        RUST_LOG              = var.core_change_publisher_rust_log
      },
      var.core_change_publisher_aws_endpoint_url != "" ? { AWS_ENDPOINT_URL = var.core_change_publisher_aws_endpoint_url } : {}
    )
  }

  vpc_config {
    subnet_ids         = var.private_subnets
    security_group_ids = [aws_security_group.ec_lambda.id]
  }

  tags = var.tags
}

resource "aws_lambda_event_source_mapping" "core_change_events" {
  event_source_arn = aws_sqs_queue.core_change_events.arn
  function_name    = aws_lambda_function.core_change_publisher.arn
  batch_size       = 10
  enabled          = true
}

resource "aws_lambda_permission" "headend_api_apigw" {
  statement_id  = "AllowExecutionFromAPIGateway"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.headend_api.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.headend_api.execution_arn}/*/*"
}
