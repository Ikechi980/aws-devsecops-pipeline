# =============================================================================
# Sentrics-Core Security Groups
# =============================================================================

resource "aws_security_group" "lambda" {
  name        = "${var.lambda_sg_name}-${var.environment}"
  description = "Lambda security group"
  vpc_id      = data.aws_vpc.this.id
  tags        = local.merged_tags
}

resource "aws_security_group" "sns_endpoint" {
  name        = "${local.name_prefix}-sns-endpoint"
  description = "SNS VPC endpoint security group"
  vpc_id      = data.aws_vpc.this.id
  tags        = local.merged_tags
}

resource "aws_security_group" "rds" {
  name        = "${var.rds_sg_name}-${var.environment}"
  description = "RDS security group"
  vpc_id      = data.aws_vpc.this.id
  tags        = local.merged_tags
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

resource "aws_security_group_rule" "sns_endpoint_ingress_from_ec_ecs_tasks" {
  type                     = "ingress"
  security_group_id        = aws_security_group.sns_endpoint.id
  from_port                = 443
  to_port                  = 443
  protocol                 = "tcp"
  source_security_group_id = aws_security_group.ecs_tasks.id
  description              = "HTTPS from ensure-cloud ECS task SG to SNS endpoint"
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

# =============================================================================
# Ensure-Cloud Security Groups
# (Terraform label ec_lambda avoids conflict with sentrics-core aws_security_group.lambda;
#  the AWS resource name "${var.project}-${var.environment}-lambda" is unchanged)
# =============================================================================

resource "aws_security_group" "ec_lambda" {
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
