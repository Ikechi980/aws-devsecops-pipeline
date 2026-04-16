# =============================================================================
# Sentrics-Core outputs
# =============================================================================

output "api_endpoint" {
  value = aws_apigatewayv2_api.this.api_endpoint
}

output "container_api_endpoint" {
  value = aws_apigatewayv2_api.iam.api_endpoint
}

output "resources_change_events_topic_arn" {
  value = aws_sns_topic.resources_change_events.arn
}

output "lambda_function_api" {
  value = aws_lambda_function.api.function_name
}

output "lambda_function_migrate" {
  value = aws_lambda_function.migrate.function_name
}

output "lambda_function_arn" {
  value = aws_lambda_function.api.arn
}

output "rds_endpoint" {
  value = aws_db_instance.this.address
}

output "rds_port" {
  value = aws_db_instance.this.port
}

output "lambda_security_group_id" {
  value = aws_security_group.lambda.id
}

output "rds_security_group_id" {
  value = aws_security_group.rds.id
}

# =============================================================================
# Ensure-Cloud outputs
# =============================================================================

output "headend_gateway_dns" {
  value = aws_lb.headend_gateway.dns_name
}

output "headend_api_endpoint" {
  value = aws_apigatewayv2_api.headend_api.api_endpoint
}

output "headend_messages_sns_topic_arn" {
  value = module.headend_messages_sns.topic_arn
}

output "core_change_events_queue_arn" {
  value = aws_sqs_queue.core_change_events.arn
}
