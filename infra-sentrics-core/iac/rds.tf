// raadom.tf 

resource "random_password" "db" {
  length  = 16
  special = true
  override_special = "!#$%&()*+,-.:;<=>?[]^_{|}~"
}



// rds.tf

resource "aws_db_subnet_group" "this" {
  name       = "${var.db_subnet_group_name}-${var.environment}"
  subnet_ids = local.private_subnet_ids
  tags       = local.merged_tags
}

resource "aws_db_parameter_group" "this" {
  name   = "${var.db_parameter_group_name}-${var.environment}"
  family = "postgres16"
  tags   = local.merged_tags
}

resource "aws_db_instance" "this" {
  identifier = "${var.database_identifier}-${var.environment}"

  engine         = "postgres"
  engine_version = var.database_engine_version

  instance_class          = var.database_instance_class
  allocated_storage       = var.database_allocated_storage_gb
  storage_encrypted       = true
  deletion_protection     = var.database_deletion_protection
  publicly_accessible     = var.database_publicly_accessible
  multi_az                = var.database_multi_az
  apply_immediately       = var.database_apply_immediately
  backup_retention_period = var.database_backup_retention_days

  db_name  = var.database_name
  username = var.database_username
  password = random_password.db.result
  port     = var.database_port

  db_subnet_group_name   = aws_db_subnet_group.this.name
  vpc_security_group_ids = [aws_security_group.rds.id]
  parameter_group_name   = aws_db_parameter_group.this.name

  skip_final_snapshot = true


  tags = local.merged_tags
}
