
# ECS Cluster
module "ecs_cluster" {
  source      = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster?ref=develop2-clean-asg-elb"
  cluster_map = var.cluster_map
  environment = var.environment

}

# Cloud Map private DNS namespace for internal ECS service discovery
resource "aws_service_discovery_private_dns_namespace" "ensure_cloud" {
  name        = "${var.project}-${var.environment}.internal"
  description = "Private namespace for ensure-cloud ECS services"
  vpc         = var.vpc_id
  tags        = var.tags
}

# Cloud Map service used by stepca ECS service registration
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

# Cloud Map service used by pki-api ECS service registration
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

#  ECS Task Security Group
resource "aws_security_group" "ecs_tasks" {
  name        = "${var.project}-${var.environment}-ecs-tasks"
  description = "ECS task security group"
  vpc_id      = var.vpc_id

  ingress {
    description = "Allow tasks to communicate within the same security group"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    self        = true
  }

  ingress {
    description     = "Allow traffic from ALB to headend-gateway"
    from_port       = 3000
    to_port         = 3000
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }

  ingress {
    description = "Allow all traffic from allowed WG/VPC CIDRs"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = var.ecs_tasks_ingress_cidrs
  }

  ingress {
    description = "Allow pki-api traffic from WireGuard CIDRs"
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = var.wg_ingress_cidrs
  }

  egress {
    description = "Allow all outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = var.tags
}

# SNS Module
module "headend_messages_sns" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/sns?ref=develop2-clean-asg-elb"
  name   = "${var.project}-${var.environment}-headend-messages"
  tags   = var.tags
}

#  ALB Security Group
resource "aws_security_group" "alb" {
  name        = "${var.project}-${var.environment}-alb"
  description = "ALB security group"
  vpc_id      = var.vpc_id

  ingress {
    description = "Allow HTTPS"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = var.alb_ingress_cidrs
  }

  egress {
    description = "Allow all outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = var.tags
}

#  Headend Gateway ALB
resource "aws_lb" "headend_gateway" {
  name               = "${var.project}-${var.environment}-headend-gw"
  internal           = false
  load_balancer_type = "application"
  subnets            = length(var.lb_subnets) > 0 ? var.lb_subnets : var.private_subnets
  security_groups    = [aws_security_group.alb.id]
  tags               = var.tags
}

#  Headend Gateway Target Group
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

#  ALB Trust Store
resource "aws_lb_trust_store" "headend_gateway" {
  name = "${var.project}-${var.environment}-headend-gw"

  ca_certificates_bundle_s3_bucket = var.alb_trust_store_bucket
  ca_certificates_bundle_s3_key    = var.alb_trust_store_key
}

#  Headend Gateway HTTPS Listener
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
#  Lambda Security Group
resource "aws_security_group" "lambda" {
  name        = "${var.project}-${var.environment}-lambda"
  description = "Lambda security group"
  vpc_id      = var.vpc_id

  egress {
    description = "Allow all outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = var.tags
}

#  Core Change Events SNS Topic
resource "aws_sns_topic" "core_change_events" {
  name              = "${var.project}-${var.environment}-core-change-events"
  kms_master_key_id = "alias/aws/sns"
  tags              = var.tags
}

#  Core Change Events SQS Queue
resource "aws_sqs_queue" "core_change_events_dlq" {
  name                      = "${var.project}-${var.environment}-core-change-events-dlq"
  message_retention_seconds = 1209600
  sqs_managed_sse_enabled   = true
  tags                      = var.tags
}

resource "aws_sqs_queue" "core_change_events" {
  name                       = "${var.project}-${var.environment}-core-change-events"
  message_retention_seconds  = 1209600
  visibility_timeout_seconds = 60
  sqs_managed_sse_enabled    = true
  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.core_change_events_dlq.arn
    maxReceiveCount     = 5
  })
  tags = var.tags
}

#  Core Change Events SQS Queue Policy
resource "aws_sqs_queue_policy" "core_change_events" {
  queue_url = aws_sqs_queue.core_change_events.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid       = "AllowEnsureCloudSnsToSendMessage"
        Effect    = "Allow"
        Principal = { Service = "sns.amazonaws.com" }
        Action    = "sqs:SendMessage"
        Resource  = aws_sqs_queue.core_change_events.arn
        Condition = {
          ArnEquals = { "aws:SourceArn" = aws_sns_topic.core_change_events.arn }
        }
      },
      {
        Sid       = "AllowSentricsCoreSnsToSendMessage"
        Effect    = "Allow"
        Principal = { Service = "sns.amazonaws.com" }
        Action    = "sqs:SendMessage"
        Resource  = aws_sqs_queue.core_change_events.arn
        Condition = {
          ArnEquals = {
            "aws:SourceArn" = data.terraform_remote_state.sentrics_core.outputs.resources_change_events_topic_arn
          }
        }
      }
    ]
  })
}

#  Core Change Events SNS Subscription
resource "aws_sns_topic_subscription" "core_change_events" {
  topic_arn = aws_sns_topic.core_change_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.core_change_events.arn
}

resource "aws_sns_topic_subscription" "sentrics_core_resources_change_events" {
  topic_arn = data.terraform_remote_state.sentrics_core.outputs.resources_change_events_topic_arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.core_change_events.arn
}


#  Headend API Lambda IAM Role
resource "aws_iam_role" "lambda_headend_api" {
  name               = "${var.project}-${var.environment}-headend-api-lambda"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume.json
  tags               = var.tags
}

#  Core Change Publisher Lambda IAM Role
resource "aws_iam_role" "lambda_core_change_publisher" {
  name               = "${var.project}-${var.environment}-core-change-publisher-lambda"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume.json
  tags               = var.tags
}

#  Headend API Lambda Basic Policy Attachment
resource "aws_iam_role_policy_attachment" "lambda_headend_api_basic" {
  role       = aws_iam_role.lambda_headend_api.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

#  Headend API Lambda VPC Policy Attachment
resource "aws_iam_role_policy_attachment" "lambda_headend_api_vpc" {
  role       = aws_iam_role.lambda_headend_api.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole"
}

#  Headend API Lambda Invoke API Gateway Policy
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

#  Core Change Publisher Lambda Basic Policy Attachment
resource "aws_iam_role_policy_attachment" "lambda_core_change_publisher_basic" {
  role       = aws_iam_role.lambda_core_change_publisher.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

#  Core Change Publisher Lambda VPC Policy Attachment
resource "aws_iam_role_policy_attachment" "lambda_core_change_publisher_vpc" {
  role       = aws_iam_role.lambda_core_change_publisher.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole"
}

#  Core Change Publisher Lambda Inline Policy
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

#  Lambda SSM Read Policy
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

#  Headend API Lambda SSM Policy Attachment
resource "aws_iam_role_policy_attachment" "lambda_headend_api_ssm" {
  role       = aws_iam_role.lambda_headend_api.name
  policy_arn = aws_iam_policy.lambda_ssm_read.arn
}

#  Core Change Publisher Lambda SSM Policy Attachment
resource "aws_iam_role_policy_attachment" "lambda_core_change_publisher_ssm" {
  role       = aws_iam_role.lambda_core_change_publisher.name
  policy_arn = aws_iam_policy.lambda_ssm_read.arn
}

#  Headend API Lambda Function
resource "aws_lambda_function" "headend_api" {
  function_name = "${var.project}-${var.environment}-headend-api"
  role          = aws_iam_role.lambda_headend_api.arn
  handler       = "bootstrap"
  runtime       = "provided.al2023"
  s3_bucket     = var.lambda_s3_bucket
  s3_key        = var.lambda_headend_api_s3_key

  timeout     = 30
  memory_size = 512

  environment {
    variables = merge(
      {
        SYSTEMS_API_BASE_URL           = var.headend_api_systems_api_base_url
        CORE_RESOURCES_API_BASE_URL    = data.terraform_remote_state.sentrics_core.outputs.container_api_endpoint
        EVENTS_MONGO_URL_SSM_PARAMETER = var.headend_api_events_mongo_url_ssm_parameter
        EVENTS_LIMIT_DEFAULT           = tostring(var.headend_api_events_limit_default)
        EVENTS_LIMIT_MAX               = tostring(var.headend_api_events_limit_max)
        RUST_LOG                       = var.headend_api_rust_log
      },
      var.headend_api_allow_unauthenticated ? { ALLOW_UNAUTHENTICATED = "1" } : {}
    )
  }

  vpc_config {
    subnet_ids         = var.private_subnets
    security_group_ids = [aws_security_group.lambda.id]
  }

  tags = var.tags
}

#  Core Change Publisher Lambda Function
resource "aws_lambda_function" "core_change_publisher" {
  function_name = "${var.project}-${var.environment}-core-change-publisher"
  role          = aws_iam_role.lambda_core_change_publisher.arn
  handler       = "bootstrap"
  runtime       = "provided.al2023"
  s3_bucket     = var.lambda_s3_bucket
  s3_key        = var.lambda_core_change_publisher_s3_key

  timeout     = 30
  memory_size = 512

  environment {
    variables = merge(
      {
        SYSTEMS_API_BASE_URL  = var.core_change_publisher_systems_api_base_url
        HEADEND_SNS_TOPIC_ARN = module.headend_messages_sns.topic_arn
        RUST_LOG              = var.core_change_publisher_rust_log
      },
      var.core_change_publisher_aws_endpoint_url != "" ? { AWS_ENDPOINT_URL = var.core_change_publisher_aws_endpoint_url } : {}
    )
  }

  vpc_config {
    subnet_ids         = var.private_subnets
    security_group_ids = [aws_security_group.lambda.id]
  }

  tags = var.tags
}

#  Core Change Events Lambda Event Source Mapping
resource "aws_lambda_event_source_mapping" "core_change_events" {
  event_source_arn = aws_sqs_queue.core_change_events.arn
  function_name    = aws_lambda_function.core_change_publisher.arn
  batch_size       = 10
  enabled          = true
}

#  Headend API Gateway HTTP API
resource "aws_apigatewayv2_api" "headend_api" {
  name                         = "${var.project}-${var.environment}-headend-api"
  protocol_type                = "HTTP"
  disable_execute_api_endpoint = true
  tags                         = var.tags
}

#  Headend API Gateway Lambda Integration
resource "aws_apigatewayv2_integration" "headend_api_lambda" {
  api_id                 = aws_apigatewayv2_api.headend_api.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.headend_api.invoke_arn
  integration_method     = "POST"
  payload_format_version = "2.0"
  timeout_milliseconds   = 30000
}

#  Headend API Gateway Proxy Route
resource "aws_apigatewayv2_route" "headend_api_proxy" {
  api_id    = aws_apigatewayv2_api.headend_api.id
  route_key = "ANY /{proxy+}"
  target    = "integrations/${aws_apigatewayv2_integration.headend_api_lambda.id}"
}

#  Headend API Gateway Root Route
resource "aws_apigatewayv2_route" "headend_api_root" {
  api_id    = aws_apigatewayv2_api.headend_api.id
  route_key = "ANY /"
  target    = "integrations/${aws_apigatewayv2_integration.headend_api_lambda.id}"
}

#  Headend API Gateway Stage
resource "aws_apigatewayv2_stage" "headend_api" {
  api_id      = aws_apigatewayv2_api.headend_api.id
  name        = "$default"
  auto_deploy = true
  tags        = var.tags
}

#  Headend API Gateway Custom Domain
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

#  Headend API Gateway API Mapping
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

#  Headend API Lambda Permission for API Gateway
resource "aws_lambda_permission" "headend_api_apigw" {
  statement_id  = "AllowExecutionFromAPIGateway"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.headend_api.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.headend_api.execution_arn}/*/*"
}


# ECS Task Definitions
resource "aws_ecs_task_definition" "ecs_task_definitions" {
  for_each = {
    for f in local.taskdef_files :
    replace(basename(f), ".json.template", "") => {
      family                        = "${var.project}-${var.environment}-${replace(basename(f), ".json.template", "")}-task"
      task_definition_template_file = "${path.module}/taskdefs/${f}"

      # If service exists in ecs_services, use those values, else defaults
      cpu     = try(local.ecs_services[replace(basename(f), ".json.template", "")].cpu, "512")
      memory  = try(local.ecs_services[replace(basename(f), ".json.template", "")].memory, "1024")
      volumes = try(local.ecs_services[replace(basename(f), ".json.template", "")].volumes, [])
    }
  }

  family                   = each.value.family
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = each.value.cpu
  memory                   = each.value.memory
  execution_role_arn       = var.ecs_execution_role_arn
  task_role_arn            = var.ecs_task_role_arn

  container_definitions = templatefile(each.value.task_definition_template_file, {
    ecs_container_image = var.ecs_task_images[each.key]
    ecs_log_group       = "/ecs/${var.environment}-${each.key}"
  })

  dynamic "volume" {
    for_each = each.value.volumes
    content {
      name = volume.value.name
      efs_volume_configuration {
        file_system_id     = volume.value.file_system_id
        transit_encryption = "ENABLED"
        authorization_config {
          access_point_id = volume.value.access_point_id
          iam             = "DISABLED"
        }
      }
    }
  }
}


# ECS Services
module "Ecs-service" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/Ecs-service?ref=develop2-clean-asg-elb"

  for_each = local.ecs_services_without_stepca

  service_name           = "${var.project}-${var.environment}-${each.key}-service"
  cluster_id             = module.ecs_cluster.cluster_arns["main"]
  task_definition_arn    = aws_ecs_task_definition.ecs_task_definitions[each.value.task_definition_key].arn
  desired_count          = each.value.desired_count
  enable_execute_command = each.value.enable_execute_command

  subnets          = each.value.subnets
  security_groups  = each.value.security_groups
  assign_public_ip = each.value.assign_public_ip

  load_balancer = try(each.value.load_balancer, [])

  environment = var.environment
  project     = var.project
  owner       = var.owner
}

# PKI API ECS service (managed directly to attach Cloud Map service discovery)
resource "aws_ecs_service" "pki_api" {
  name                   = "${var.project}-${var.environment}-pki-api-service"
  cluster                = module.ecs_cluster.cluster_arns["main"]
  task_definition        = aws_ecs_task_definition.ecs_task_definitions["pki-api"].arn
  desired_count          = local.ecs_services["pki-api"].desired_count
  launch_type            = "FARGATE"
  enable_execute_command = local.ecs_services["pki-api"].enable_execute_command

  service_registries {
    registry_arn = aws_service_discovery_service.pki_api.arn
  }

  network_configuration {
    subnets          = local.ecs_services["pki-api"].subnets
    security_groups  = local.ecs_services["pki-api"].security_groups
    assign_public_ip = local.ecs_services["pki-api"].assign_public_ip
  }

  deployment_minimum_healthy_percent = 50
  deployment_maximum_percent         = 200

  tags = merge(
    {
      Environment = var.environment
      Project     = var.project
      Owner       = var.owner
    },
    var.tags
  )
}

# StepCA ECS service (managed directly to attach Cloud Map service discovery)
resource "aws_ecs_service" "stepca" {
  name                          = "${var.project}-${var.environment}-stepca-service"
  cluster                       = module.ecs_cluster.cluster_arns["main"]
  task_definition               = aws_ecs_task_definition.ecs_task_definitions["stepca"].arn
  desired_count                 = local.ecs_services["stepca"].desired_count
  launch_type                   = "FARGATE"
  enable_execute_command        = local.ecs_services["stepca"].enable_execute_command
  availability_zone_rebalancing = "DISABLED"

  service_registries {
    registry_arn = aws_service_discovery_service.stepca.arn
  }

  network_configuration {
    subnets          = local.ecs_services["stepca"].subnets
    security_groups  = local.ecs_services["stepca"].security_groups
    assign_public_ip = local.ecs_services["stepca"].assign_public_ip
  }

  deployment_minimum_healthy_percent = 0
  deployment_maximum_percent         = 100

  tags = merge(
    {
      Environment = var.environment
      Project     = var.project
      Owner       = var.owner
    },
    var.tags
  )
}

# ECS CloudWatch Logs
module "ecs_logs" {
  source            = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/ecs-cluster/ecs-logs?ref=develop2-clean-asg-elb"
  services          = local.ecs_log_group_names
  retention_in_days = var.log_retention
  tags              = var.tags
}


# Autoscaling:

module "autoscaling" {
  source = "git::https://github.com/SilversphereInc/iac.git//terraform/modules/autoscaling?ref=develop2-clean-asg-elb"

  service_namespace  = "ecs"
  resource_id        = "service/${var.cluster_map["main"].name}/${module.Ecs-service["headend-gateway"].service_name}"
  scalable_dimension = "ecs:service:DesiredCount"

  min_capacity = var.min_capacity
  max_capacity = var.max_capacity

  cpu_target_value    = var.cpu_target_value
  memory_target_value = var.memory_target_value

  scale_in_cooldown  = var.scale_in_cooldown
  scale_out_cooldown = var.scale_out_cooldown

  tags       = var.tags
  depends_on = [module.Ecs-service["headend-gateway"]]
}
