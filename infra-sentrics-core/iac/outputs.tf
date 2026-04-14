// outputs.tf

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
