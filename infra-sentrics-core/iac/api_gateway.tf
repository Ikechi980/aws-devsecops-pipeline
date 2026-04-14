// api_gateway.tf

resource "aws_apigatewayv2_api" "this" {
  name          = "${var.api_name}-${var.environment}"
  protocol_type = "HTTP"
  tags          = local.merged_tags

  cors_configuration {
    allow_origins = [
      "https://staging.d21ho8nexwbl7z.amplifyapp.com"
    ]

    allow_methods = [
      "GET",
      "POST",
      "PUT",
      "DELETE",
      "OPTIONS"
    ]

    allow_headers = [
      "authorization",
      "content-type"
    ]
  }
}

resource "aws_apigatewayv2_integration" "lambda" {
  api_id                 = aws_apigatewayv2_api.this.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.api.arn
  payload_format_version = "2.0"
}

resource "aws_apigatewayv2_authorizer" "jwt" {
  count = var.enable_jwt_auth ? 1 : 0

  api_id          = aws_apigatewayv2_api.this.id
  authorizer_type = "JWT"
  name            = "${var.api_name}-${var.environment}-jwt"

  identity_sources = ["$request.header.Authorization"]

  jwt_configuration {
    issuer   = var.azure_ad_jwt_issuer
    audience = [var.azure_ad_jwt_audience]
  }
}

# OPTIONS route must exist explicitly and MUST NOT require auth
resource "aws_apigatewayv2_route" "options" {
  api_id    = aws_apigatewayv2_api.this.id
  route_key = "OPTIONS /{proxy+}"
  target    = "integrations/${aws_apigatewayv2_integration.lambda.id}"

  authorization_type = "NONE"
}

# Default route remains protected by JWT
resource "aws_apigatewayv2_route" "default" {
  api_id    = aws_apigatewayv2_api.this.id
  route_key = "$default"
  target    = "integrations/${aws_apigatewayv2_integration.lambda.id}"

  authorization_type = var.enable_jwt_auth ? "JWT" : "NONE"
  authorizer_id      = var.enable_jwt_auth ? aws_apigatewayv2_authorizer.jwt[0].id : null
}

resource "aws_apigatewayv2_stage" "this" {
  api_id      = aws_apigatewayv2_api.this.id
  name        = "$default"
  auto_deploy = true
  tags        = local.merged_tags
}

resource "aws_lambda_permission" "allow_apigw" {
  statement_id  = "AllowExecutionFromHttpApi"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.api.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.this.execution_arn}/*/*"
}


############################################
# IAM-Authenticated HTTP API (Internal) Second API
############################################

resource "aws_apigatewayv2_api" "iam" {
  name          = "${var.api_iam_name}-${var.environment}"
  protocol_type = "HTTP"
  tags          = local.merged_tags

  cors_configuration {
    allow_origins = ["*"]

    allow_methods = [
      "GET",
      "POST",
      "PUT",
      "DELETE",
      "OPTIONS"
    ]

    allow_headers = [
      "authorization",
      "content-type",
      "x-amz-date",
      "x-amz-security-token"
    ]
  }
}

############################################
# Lambda Integration (same Lambda)
############################################

resource "aws_apigatewayv2_integration" "iam_lambda" {
  api_id                 = aws_apigatewayv2_api.iam.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.api.arn
  payload_format_version = "2.0"
}

############################################
# OPTIONS Route (no auth)
############################################

resource "aws_apigatewayv2_route" "iam_options" {
  api_id    = aws_apigatewayv2_api.iam.id
  route_key = "OPTIONS /{proxy+}"
  target    = "integrations/${aws_apigatewayv2_integration.iam_lambda.id}"

  authorization_type = "NONE"
}

############################################
# Default Route (AWS IAM auth)
############################################

resource "aws_apigatewayv2_route" "iam_default" {
  api_id    = aws_apigatewayv2_api.iam.id
  route_key = "$default"
  target    = "integrations/${aws_apigatewayv2_integration.iam_lambda.id}"

  authorization_type = "AWS_IAM"
}

############################################
# Stage
############################################

resource "aws_apigatewayv2_stage" "iam" {
  api_id      = aws_apigatewayv2_api.iam.id
  name        = "$default"
  auto_deploy = true
  tags        = local.merged_tags
}

############################################
# Lambda Permission for IAM API
############################################

resource "aws_lambda_permission" "allow_apigw_iam" {
  statement_id  = "AllowExecutionFromHttpApiIAM"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.api.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.iam.execution_arn}/*/*"
}
