// lambda.tf

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
    subnet_ids = local.private_subnet_ids
    security_group_ids = [aws_security_group.lambda.id]
  }

  environment {
    variables = {
      RUST_LOG     = "info"
      SNS_TOPIC_ARN = aws_sns_topic.resources_change_events.arn
      DATABASE_URL = "postgres://${var.database_username}:${urlencode(random_password.db.result)}@${aws_db_instance.this.address}:${var.database_port}/${var.database_name}"
    }
  }


  tags = local.merged_tags

  depends_on = [
    aws_iam_role_policy_attachment.lambda_basic_logs,
    aws_iam_role_policy_attachment.lambda_vpc_access
  ]
}

## Migrate function

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
    subnet_ids = local.private_subnet_ids
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

## Resources change logger (SQS-triggered)

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

  depends_on = [
    aws_iam_role_policy_attachment.lambda_basic_logs
  ]
}

resource "aws_lambda_event_source_mapping" "resources_change_logger" {
  event_source_arn       = aws_sqs_queue.resources_change_logger.arn
  function_name          = aws_lambda_function.resources_change_logger.arn
  batch_size             = var.change_logger_batch_size
  enabled                = true
  function_response_types = ["ReportBatchItemFailures"]
}
