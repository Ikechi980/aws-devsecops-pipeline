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
  service_name        = "com.amazonaws.${var.aws_region}.sns"
  vpc_endpoint_type   = "Interface"
  subnet_ids          = local.private_subnet_ids
  security_group_ids  = [aws_security_group.sns_endpoint.id]
  private_dns_enabled = true

  tags = local.merged_tags
}

