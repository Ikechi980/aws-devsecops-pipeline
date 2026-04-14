resource "aws_sqs_queue" "resources_change_logger_dlq" {
  name = "${var.change_logger_dlq_name}-${var.environment}"
  tags = local.merged_tags
}

resource "aws_sqs_queue" "resources_change_logger" {
  name                       = "${var.change_logger_queue_name}-${var.environment}"
  visibility_timeout_seconds = var.change_logger_queue_visibility_timeout_seconds
  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.resources_change_logger_dlq.arn
    maxReceiveCount     = var.change_logger_max_receive_count
  })
  tags = local.merged_tags
}

resource "aws_sqs_queue" "yardi_sync_dlq" {
  name = "${var.yardi_sync_dlq_name}-${var.environment}"
  tags = local.merged_tags
}

resource "aws_sqs_queue" "yardi_sync" {
  name                       = "${var.yardi_sync_queue_name}-${var.environment}"
  visibility_timeout_seconds = var.change_logger_queue_visibility_timeout_seconds
  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.yardi_sync_dlq.arn
    maxReceiveCount     = var.change_logger_max_receive_count
  })
  tags = local.merged_tags
}

resource "aws_sqs_queue_policy" "resources_change_logger" {
  queue_url = aws_sqs_queue.resources_change_logger.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AllowSnsPublish"
        Effect = "Allow"
        Principal = {
          Service = "sns.amazonaws.com"
        }
        Action   = "sqs:SendMessage"
        Resource = aws_sqs_queue.resources_change_logger.arn
        Condition = {
          ArnEquals = {
            "aws:SourceArn" = aws_sns_topic.resources_change_events.arn
          }
        }
      }
    ]
  })
}

resource "aws_sqs_queue_policy" "yardi_sync" {
  queue_url = aws_sqs_queue.yardi_sync.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AllowSnsPublish"
        Effect = "Allow"
        Principal = {
          Service = "sns.amazonaws.com"
        }
        Action   = "sqs:SendMessage"
        Resource = aws_sqs_queue.yardi_sync.arn
        Condition = {
          ArnEquals = {
            "aws:SourceArn" = aws_sns_topic.resources_change_events.arn
          }
        }
      }
    ]
  })
}

resource "aws_sns_topic_subscription" "resources_change_logger_sqs" {
  topic_arn            = aws_sns_topic.resources_change_events.arn
  protocol             = "sqs"
  endpoint             = aws_sqs_queue.resources_change_logger.arn
  raw_message_delivery = false
  depends_on           = [aws_sqs_queue_policy.resources_change_logger]
}

resource "aws_sns_topic_subscription" "yardi_sync_sqs" {
  topic_arn            = aws_sns_topic.resources_change_events.arn
  protocol             = "sqs"
  endpoint             = aws_sqs_queue.yardi_sync.arn
  raw_message_delivery = false
  depends_on           = [aws_sqs_queue_policy.yardi_sync]
}
