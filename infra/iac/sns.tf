# =============================================================================
# Sentrics-Core SNS
# =============================================================================

resource "aws_sns_topic" "resources_change_events" {
  name = "${var.resources_change_events_topic_name}-${var.environment}"
  tags = local.merged_tags
}

resource "aws_sns_topic" "yardi_sync_failures" {
  name = "${var.yardi_sync_failures_topic_name}-${var.environment}"
  tags = local.merged_tags
}

resource "aws_vpc_endpoint" "sns" {
  vpc_id              = data.aws_vpc.this.id
  service_name        = "com.amazonaws.${var.region}.sns"
  vpc_endpoint_type   = "Interface"
  subnet_ids          = local.private_subnet_ids
  security_group_ids  = [aws_security_group.sns_endpoint.id]
  private_dns_enabled = true
  tags                = local.merged_tags
}

# =============================================================================
# Ensure-Cloud SNS
# =============================================================================

module "headend_messages_sns" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/sns?ref=develop2-clean-asg-elb"
  name   = "${var.project}-${var.environment}-headend-messages"
  tags   = var.tags
}

resource "aws_sns_topic" "core_change_events" {
  name              = "${var.project}-${var.environment}-core-change-events"
  kms_master_key_id = "alias/aws/sns"
  tags              = var.tags
}

resource "aws_sns_topic_subscription" "core_change_events" {
  topic_arn = aws_sns_topic.core_change_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.core_change_events.arn
}

# Cross-stack fix: was data.terraform_remote_state.sentrics_core.outputs.resources_change_events_topic_arn
resource "aws_sns_topic_subscription" "sentrics_core_resources_change_events" {
  topic_arn = aws_sns_topic.resources_change_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.core_change_events.arn
}
