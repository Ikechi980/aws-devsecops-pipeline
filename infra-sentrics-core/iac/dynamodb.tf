resource "aws_dynamodb_table" "resources_change_log" {
  name         = "${var.change_log_table_name}-${var.environment}"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "community_pk"
  range_key    = "timestamp_sk"
  deletion_protection_enabled = false

  attribute {
    name = "community_pk"
    type = "S"
  }

  attribute {
    name = "timestamp_sk"
    type = "S"
  }

  attribute {
    name = "resource_pk"
    type = "S"
  }

  attribute {
    name = "requester_pk"
    type = "S"
  }

  global_secondary_index {
    name            = "by_resource"
    hash_key        = "resource_pk"
    range_key       = "timestamp_sk"
    projection_type = "ALL"
  }

  global_secondary_index {
    name            = "by_requester"
    hash_key        = "requester_pk"
    range_key       = "timestamp_sk"
    projection_type = "ALL"
  }

  tags = local.merged_tags
}
