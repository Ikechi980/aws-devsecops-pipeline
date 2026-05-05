# =============================================================================
# GuardDuty — Runtime Security
#
# Enables GuardDuty with ECS Fargate Runtime Monitoring and Lambda Protection.
# AWS automatically injects a security agent sidecar into every ECS Fargate
# task — no Dockerfile changes required. Findings are forwarded to SNS via
# an EventBridge rule so the team is alerted immediately.
# =============================================================================

resource "aws_guardduty_detector" "this" {
  enable = true

  datasources {
    s3_logs {
      enable = true
    }
    kubernetes {
      audit_logs {
        enable = false
      }
    }
    malware_protection {
      scan_ec2_instance_with_findings {
        ebs_volumes {
          enable = true
        }
      }
    }
  }

  tags = local.merged_tags
}

# ECS Fargate Runtime Monitoring — AWS injects the agent automatically
resource "aws_guardduty_detector_feature" "ecs_runtime_monitoring" {
  detector_id = aws_guardduty_detector.this.id
  name        = "RUNTIME_MONITORING"
  status      = "ENABLED"

  additional_configuration {
    name   = "ECS_FARGATE_AGENT_MANAGEMENT"
    status = "ENABLED"
  }
}

# Lambda Protection — monitors Lambda execution behaviour
resource "aws_guardduty_detector_feature" "lambda_protection" {
  detector_id = aws_guardduty_detector.this.id
  name        = "LAMBDA_NETWORK_LOGS"
  status      = "ENABLED"
}

# =============================================================================
# GuardDuty Findings → SNS alert
# =============================================================================

resource "aws_sns_topic" "guardduty_findings" {
  name = "${var.project_name}-${var.environment}-guardduty-findings"
  tags = local.merged_tags
}

resource "aws_sns_topic_policy" "guardduty_findings" {
  arn = aws_sns_topic.guardduty_findings.arn
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect    = "Allow"
        Principal = { Service = "events.amazonaws.com" }
        Action    = "sns:Publish"
        Resource  = aws_sns_topic.guardduty_findings.arn
      }
    ]
  })
}

# EventBridge rule — fires on every GuardDuty HIGH or CRITICAL finding
resource "aws_cloudwatch_event_rule" "guardduty_findings" {
  name        = "${var.project_name}-${var.environment}-guardduty-findings"
  description = "Forward HIGH and CRITICAL GuardDuty findings to SNS"

  event_pattern = jsonencode({
    source      = ["aws.guardduty"]
    detail-type = ["GuardDuty Finding"]
    detail = {
      severity = [{ numeric = [">=", 7] }]
    }
  })

  tags = local.merged_tags
}

resource "aws_cloudwatch_event_target" "guardduty_findings_sns" {
  rule      = aws_cloudwatch_event_rule.guardduty_findings.name
  target_id = "guardduty-findings-sns"
  arn       = aws_sns_topic.guardduty_findings.arn

  input_transformer {
    input_paths = {
      severity    = "$.detail.severity"
      type        = "$.detail.type"
      description = "$.detail.description"
      account     = "$.detail.accountId"
      region      = "$.region"
      time        = "$.time"
    }
    input_template = "\"GuardDuty Finding\\nSeverity : <severity>\\nType     : <type>\\nAccount  : <account>\\nRegion   : <region>\\nTime     : <time>\\nDetails  : <description>\""
  }
}
