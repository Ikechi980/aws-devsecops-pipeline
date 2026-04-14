// iam.tf

data "aws_iam_policy_document" "lambda_assume_role" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}

resource "aws_iam_role_policy" "lambda_sns_publish" {
  name = "${var.lambda_sns_policy_name}-${var.environment}"
  role = aws_iam_role.lambda_exec.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "sns:Publish"
        ]
        Resource = aws_sns_topic.resources_change_events.arn
      }
    ]
  })
}


resource "aws_iam_role" "lambda_exec" {
  name               = "${var.lambda_exec_role_name}-${var.environment}"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume_role.json
  tags               = local.merged_tags
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
        Effect = "Allow"
        Action = [
          "dynamodb:PutItem"
        ]
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
