data "aws_vpc" "this" {
  filter {
    name   = "tag:Name"
    values = ["Ensure-VPC-Production"]
  }
}

data "aws_subnet" "private_1" {
  filter {
    name   = "tag:Name"
    values = ["Ensure-private-sub-1"]
  }

  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.this.id]
  }
}

data "aws_subnet" "private_2" {
  filter {
    name   = "tag:Name"
    values = ["Ensure-private-sub-2"]
  }

  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.this.id]
  }
}


