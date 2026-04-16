# =============================================================================
# Sentrics-Core IAM
# =============================================================================

resource "aws_iam_role" "lambda_exec" {
  name               = "${var.lambda_exec_role_name}-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume_role.json
  tags               = local.merged_tags
}

resource "aws_iam_role_policy" "lambda_sns_publish" {
  name = "${var.lambda_sns_policy_name}-${var.environment}"
  role = aws_iam_role.lambda_exec.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["sns:Publish"]
        Resource = aws_sns_topic.resources_change_events.arn
      }
    ]
  })
}

resource "aws_iam_role_policy_attachment" "lambda_basic_logs" {
  role       = aws_iam_role.lambda_exec.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

resource "aws_iam_role_policy_attachment" "lambda_vpc_access" {
  role       = aws_iam_role.lambda_exec.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole"
}

resource "aws_iam_role_policy" "change_logger_access" {
  name = "${var.change_logger_iam_policy_name}-${var.environment}"
  role = aws_iam_role.lambda_exec.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["dynamodb:PutItem"]
        Resource = aws_dynamodb_table.resources_change_log.arn
      },
      {
        Effect = "Allow"
        Action = [
          "sqs:ReceiveMessage",
          "sqs:DeleteMessage",
          "sqs:GetQueueAttributes"
        ]
        Resource = aws_sqs_queue.resources_change_logger.arn
      }
    ]
  })
}

# =============================================================================
# Ensure-Cloud IAM
# =============================================================================

resource "aws_iam_role" "lambda_headend_api" {
  name               = "${var.project}-${var.environment}-headend-api-lambda"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume_role.json
  tags               = var.tags
}

resource "aws_iam_role" "lambda_core_change_publisher" {
  name               = "${var.project}-${var.environment}-core-change-publisher-lambda"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume_role.json
  tags               = var.tags
}

resource "aws_iam_role_policy_attachment" "lambda_headend_api_basic" {
  role       = aws_iam_role.lambda_headend_api.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

resource "aws_iam_role_policy_attachment" "lambda_headend_api_vpc" {
  role       = aws_iam_role.lambda_headend_api.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole"
}

resource "aws_iam_role_policy" "lambda_headend_api_execute_api_invoke" {
  name = "${var.project}-${var.environment}-headend-api-execute-api-invoke"
  role = aws_iam_role.lambda_headend_api.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["execute-api:Invoke"]
        Resource = "*"
      }
    ]
  })
}

resource "aws_iam_role_policy_attachment" "lambda_core_change_publisher_basic" {
  role       = aws_iam_role.lambda_core_change_publisher.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

resource "aws_iam_role_policy_attachment" "lambda_core_change_publisher_vpc" {
  role       = aws_iam_role.lambda_core_change_publisher.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole"
}

resource "aws_iam_role_policy" "lambda_core_change_publisher" {
  name = "${var.project}-${var.environment}-core-change-publisher"
  role = aws_iam_role.lambda_core_change_publisher.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["sns:Publish"]
        Resource = module.headend_messages_sns.topic_arn
      },
      {
        Effect = "Allow"
        Action = [
          "sqs:ReceiveMessage",
          "sqs:DeleteMessage",
          "sqs:GetQueueAttributes"
        ]
        Resource = aws_sqs_queue.core_change_events.arn
      }
    ]
  })
}

resource "aws_iam_policy" "lambda_ssm_read" {
  name = "${var.project}-${var.environment}-lambda-ssm-read"
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "ssm:GetParameter",
          "ssm:GetParameters"
        ]
        Resource = [
          "arn:aws:ssm:${var.region}:${data.aws_caller_identity.current.account_id}:parameter/ensure-cloud/headend-api/*",
          "arn:aws:ssm:${var.region}:${data.aws_caller_identity.current.account_id}:parameter/ensure-cloud/core-change-publisher/*"
        ]
      },
      {
        Effect   = "Allow"
        Action   = ["kms:Decrypt"]
        Resource = "*"
      }
    ]
  })
}

resource "aws_iam_role_policy_attachment" "lambda_headend_api_ssm" {
  role       = aws_iam_role.lambda_headend_api.name
  policy_arn = aws_iam_policy.lambda_ssm_read.arn
}

resource "aws_iam_role_policy_attachment" "lambda_core_change_publisher_ssm" {
  role       = aws_iam_role.lambda_core_change_publisher.name
  policy_arn = aws_iam_policy.lambda_ssm_read.arn
}
