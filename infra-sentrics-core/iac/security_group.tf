// security_group.tf

resource "aws_security_group" "lambda" {
  name        = "${var.lambda_sg_name}-${var.environment}"
  description = "Lambda security group"
  vpc_id      = data.aws_vpc.this.id

  tags = local.merged_tags
}

resource "aws_security_group" "sns_endpoint" {
  name        = "${local.name_prefix}-sns-endpoint"
  description = "SNS VPC endpoint security group"
  vpc_id      = data.aws_vpc.this.id

  tags = local.merged_tags
}

resource "aws_security_group" "rds" {
  name        = "${var.rds_sg_name}-${var.environment}"
  description = "RDS security group"
  vpc_id      = data.aws_vpc.this.id

  tags = local.merged_tags
}

resource "aws_security_group_rule" "rds_ingress_from_lambda" {
  type                     = "ingress"
  security_group_id        = aws_security_group.rds.id
  from_port                = var.database_port
  to_port                  = var.database_port
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.lambda.id
  description              = "Postgres from Lambda SG"
}

resource "aws_security_group_rule" "lambda_egress_to_rds" {
  type                     = "egress"
  security_group_id        = aws_security_group.lambda.id
  from_port                = var.database_port
  to_port                  = var.database_port
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.rds.id
  description              = "Postgres to RDS SG"
}

resource "aws_security_group_rule" "sns_endpoint_ingress_from_lambda" {
  type                     = "ingress"
  security_group_id        = aws_security_group.sns_endpoint.id
  from_port                = 443
  to_port                  = 443
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.lambda.id
  description              = "HTTPS from Lambda SG to SNS endpoint"
}

resource "aws_security_group_rule" "sns_endpoint_ingress_from_ecs_tasks" {
  for_each                 = toset(var.ecs_security_groups)
  type                     = "ingress"
  security_group_id        = aws_security_group.sns_endpoint.id
  from_port                = 443
  to_port                  = 443
  protocol                 = "tcp"
  source_security_group_id = each.value
  description              = "HTTPS from ECS task SG to SNS endpoint"
}

resource "aws_security_group_rule" "lambda_ingress_from_sns_endpoint" {
  type                     = "ingress"
  security_group_id        = aws_security_group.lambda.id
  from_port                = 443
  to_port                  = 443
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.sns_endpoint.id
  description              = "HTTPS from SNS endpoint SG"
}

resource "aws_security_group_rule" "lambda_egress_to_sns_endpoint" {
  type                     = "egress"
  security_group_id        = aws_security_group.lambda.id
  from_port                = 443
  to_port                  = 443
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.sns_endpoint.id
  description              = "HTTPS to SNS endpoint SG"
}
