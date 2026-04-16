# =============================================================================
# Ensure-Cloud — Service Discovery, ALB, API Gateway, Route53
# =============================================================================

resource "aws_service_discovery_private_dns_namespace" "ensure_cloud" {
  name        = "${var.project}-${var.environment}.internal"
  description = "Private namespace for ensure-cloud ECS services"
  vpc         = var.vpc_id
  tags        = var.tags
}

resource "aws_service_discovery_service" "stepca" {
  name = "${var.environment}-stepca"

  dns_config {
    namespace_id   = aws_service_discovery_private_dns_namespace.ensure_cloud.id
    routing_policy = "MULTIVALUE"
    dns_records {
      type = "A"
      ttl  = 10
    }
  }

  tags = var.tags
}

resource "aws_service_discovery_service" "pki_api" {
  name = "${var.environment}-pki-api"

  dns_config {
    namespace_id   = aws_service_discovery_private_dns_namespace.ensure_cloud.id
    routing_policy = "MULTIVALUE"
    dns_records {
      type = "A"
      ttl  = 10
    }
  }

  tags = var.tags
}

resource "aws_lb" "headend_gateway" {
  name               = "${var.project}-${var.environment}-headend-gw"
  internal           = false
  load_balancer_type = "application"
  subnets            = length(var.lb_subnets) > 0 ? var.lb_subnets : var.private_subnets
  security_groups    = [aws_security_group.alb.id]
  tags               = var.tags
}

resource "aws_lb_target_group" "headend_gateway" {
  name        = "${var.project}-${var.environment}-headend-gw"
  port        = 3000
  protocol    = "HTTP"
  vpc_id      = var.vpc_id
  target_type = "ip"

  health_check {
    path                = "/v1/health"
    protocol            = "HTTP"
    matcher             = "200-399"
    interval            = 30
    timeout             = 5
    healthy_threshold   = 2
    unhealthy_threshold = 3
  }

  tags = var.tags
}

resource "aws_lb_trust_store" "headend_gateway" {
  name                             = "${var.project}-${var.environment}-headend-gw"
  ca_certificates_bundle_s3_bucket = var.alb_trust_store_bucket
  ca_certificates_bundle_s3_key    = var.alb_trust_store_key
}

resource "aws_lb_listener" "headend_gateway_https" {
  load_balancer_arn = aws_lb.headend_gateway.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = var.alb_certificate_arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.headend_gateway.arn
  }

  mutual_authentication {
    mode            = "verify"
    trust_store_arn = aws_lb_trust_store.headend_gateway.arn
  }
}

resource "aws_route53_record" "headend_gateway_public" {
  count   = var.alb_custom_domain_name != "" ? 1 : 0
  zone_id = data.aws_route53_zone.public.zone_id
  name    = var.alb_custom_domain_name
  type    = "A"

  alias {
    name                   = aws_lb.headend_gateway.dns_name
    zone_id                = aws_lb.headend_gateway.zone_id
    evaluate_target_health = true
  }
}

resource "aws_apigatewayv2_api" "headend_api" {
  name                         = "${var.project}-${var.environment}-headend-api"
  protocol_type                = "HTTP"
  disable_execute_api_endpoint = true
  tags                         = var.tags
}

resource "aws_apigatewayv2_integration" "headend_api_lambda" {
  api_id                 = aws_apigatewayv2_api.headend_api.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.headend_api.invoke_arn
  integration_method     = "POST"
  payload_format_version = "2.0"
  timeout_milliseconds   = 30000
}

resource "aws_apigatewayv2_route" "headend_api_proxy" {
  api_id    = aws_apigatewayv2_api.headend_api.id
  route_key = "ANY /{proxy+}"
  target    = "integrations/${aws_apigatewayv2_integration.headend_api_lambda.id}"
}

resource "aws_apigatewayv2_route" "headend_api_root" {
  api_id    = aws_apigatewayv2_api.headend_api.id
  route_key = "ANY /"
  target    = "integrations/${aws_apigatewayv2_integration.headend_api_lambda.id}"
}

resource "aws_apigatewayv2_stage" "headend_api" {
  api_id      = aws_apigatewayv2_api.headend_api.id
  name        = "$default"
  auto_deploy = true
  tags        = var.tags
}

resource "aws_apigatewayv2_domain_name" "headend_api" {
  domain_name = var.apigw_custom_domain_name

  domain_name_configuration {
    certificate_arn = var.apigw_certificate_arn
    endpoint_type   = "REGIONAL"
    security_policy = "TLS_1_2"
  }

  mutual_tls_authentication {
    truststore_uri = "s3://${var.apigw_trust_store_bucket}/${var.apigw_trust_store_key}"
  }

  tags = var.tags
}

resource "aws_apigatewayv2_api_mapping" "headend_api" {
  api_id      = aws_apigatewayv2_api.headend_api.id
  domain_name = aws_apigatewayv2_domain_name.headend_api.id
  stage       = aws_apigatewayv2_stage.headend_api.id
}

resource "aws_route53_record" "headend_api_custom_domain" {
  zone_id = data.aws_route53_zone.public.zone_id
  name    = var.apigw_custom_domain_name
  type    = "A"

  alias {
    name                   = aws_apigatewayv2_domain_name.headend_api.domain_name_configuration[0].target_domain_name
    zone_id                = aws_apigatewayv2_domain_name.headend_api.domain_name_configuration[0].hosted_zone_id
    evaluate_target_health = false
  }
}
