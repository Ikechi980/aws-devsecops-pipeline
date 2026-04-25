# Infrastructure — Terraform IaC

This directory contains the Terraform configuration for the entire Sentrics Ensure platform. Both the `sentrics-core` and `ensure-cloud` stacks are managed in a single state file to simplify cross-stack resource references.

---

## What Gets Provisioned

### sentrics-core

| Resource | Details |
|----------|---------|
| Lambda — `resources-api` | Public REST API, ARM64, `provided.al2023` |
| Lambda — `migrate` | Database migration runner, invoked on each deploy |
| Lambda — `resources-change-logger` | Consumes SQS, writes audit records to DynamoDB |
| ECS Cluster — `sentrics-core-cluster` | Container Insights enabled |
| ECS Service — `yardi-sync` | Yardi EHR sync background service |
| RDS — PostgreSQL 16 | Private subnet only, encrypted at rest, password in SSM |
| SNS — `resources-change-events` | Published by resources-api on every resource mutation |
| SNS — `yardi-sync-failures` | Published by yardi-sync on integration errors |
| SQS — `yardi-sync-queue` | Subscribed to resources-change-events |
| SQS — `resources-change-logger-queue` | Subscribed to resources-change-events |
| DynamoDB — `resources-change-log` | Audit trail for all resource change events |
| API Gateway v2 — public | JWT-authenticated (Azure AD), custom domain |
| API Gateway v2 — internal | IAM-authenticated, used by ensure-cloud services |
| SSM SecureString | Database URL stored at `/sentrics-core/{env}/database-url` |

### ensure-cloud

| Resource | Details |
|----------|---------|
| Lambda — `headend-api` | Headend-facing HTTP API, reads SSM for MongoDB URL |
| Lambda — `core-change-publisher` | Consumes SQS, publishes to headend SNS topic |
| ECS Cluster — `ensure-cloud-{env}` | Container Insights enabled |
| ECS Service — `headend-gateway` | WebSocket gateway for headend devices |
| ECS Service — `pki-api` | Certificate issuance API |
| ECS Service — `stepca` | Private CA, backed by EFS for persistent state |
| ALB | mTLS required, custom domain, trust store in S3 |
| API Gateway v2 | mTLS + JWT auth, custom domain |
| SNS — `headend-messages` | Outbound events to headend communities |
| SQS — `core-change-events` | Feeds core-change-publisher |
| Route53 + ACM | Custom domains for ALB and API Gateway |
| Auto Scaling | ECS services scale on CPU and memory targets |

---

## Directory Layout

```
infra/
└── iac/
    ├── backend.tf              # Remote state backend (S3)
    ├── versions.tf             # Terraform + provider version constraints
    ├── providers.tf            # AWS provider configuration
    ├── variables.tf            # All input variables (shared + per-stack)
    ├── locals.tf               # Computed local values
    ├── data-sources.tf         # VPC, subnets, caller identity lookups
    ├── main.tf                 # ALB, API Gateway, Route53, service discovery
    ├── lambda.tf               # All Lambda function definitions
    ├── ecs-cluster.tf          # ECS clusters and services
    ├── rds.tf                  # RDS instance + SSM secret for database URL
    ├── iam.tf                  # IAM roles and policies (least-privilege)
    ├── security_group.tf       # Security groups
    ├── api_gateway.tf          # API Gateway v2 routes and authorisers
    ├── sns.tf                  # SNS topics
    ├── sqs.tf                  # SQS queues and DLQs
    ├── dynamodb.tf             # DynamoDB tables
    ├── outputs.tf              # Stack outputs
    ├── dev.tfvars              # Development environment values
    ├── prod.tfvars             # Production environment values
    ├── dev-backend.tfvars      # Dev remote state backend config
    ├── prod-backend.tfvars     # Prod remote state backend config
    └── taskdefs/               # ECS task definition templates
        ├── headend-gateway.json.template
        ├── pki-api.json.template
        └── stepca.json.template
```

---

## Remote State Backend

State is stored in S3 with a separate backend config per environment. The backend block in `backend.tf` is intentionally empty — values are supplied at `init` time via `-backend-config`.

| Environment | Backend config |
|-------------|---------------|
| dev | `dev-backend.tfvars` |
| prod | `prod-backend.tfvars` |

---

## Running via Jenkins

Infrastructure changes are applied through `Jenkinsfile.infra` at the repo root. This is the preferred and safest method — it requires an interactive approval step before `apply` or `destroy`.

**Pipeline parameters:**

| Parameter | Values | Description |
|-----------|--------|-------------|
| `ENVIRONMENT` | `dev`, `prod` | Target environment |
| `ACTION` | `plan`, `apply`, `destroy` | Terraform action |
| `RELEASE_SHA` | commit SHA | Required for `apply` — ties infra to a specific build |

**Workflow:**
1. Raise a PR to `development` — the [infra security gate](../.github/workflows/infra-gates.yml) runs Trivy config and Gitleaks
2. PR merges after gate passes
3. Trigger `Jenkinsfile.infra` in Jenkins — select `ENVIRONMENT` and `ACTION`
4. For `apply` — Jenkins runs `plan` first, then pauses for manual approval before applying

---

## Running Locally

> Only use this for inspection and dry-runs. All production changes must go through the Jenkins pipeline.

**Prerequisites:**
- Terraform ≥ 1.7.0
- AWS CLI configured with appropriate credentials
- Access to the remote state S3 bucket

```bash
cd infra/iac

# Initialise with dev backend
terraform init -backend-config=dev-backend.tfvars

# Plan — review changes before applying
terraform plan -var-file=dev.tfvars

# Apply (requires explicit confirmation)
terraform apply -var-file=dev.tfvars
```

For production:
```bash
terraform init -backend-config=prod-backend.tfvars
terraform plan -var-file=prod.tfvars
```

---

## IAM Design

Every Lambda and ECS task has its own IAM role scoped to the exact actions and resources it needs:

- `resources-api` and `migrate` — SNS publish, SSM GetParameter (database URL only), VPC access
- `resources-change-logger` — SQS receive/delete, DynamoDB PutItem
- `headend-api` — SSM GetParameter (MongoDB URL only), execute-api invoke, VPC access
- `core-change-publisher` — SNS publish, SQS receive/delete, SSM read
- ECS task roles — inherited from the shared ECS execution role with service-specific additions

No wildcard resource ARNs are used in inline policies.

---

## Secrets Management

Database credentials are never stored in Lambda environment variables. The full PostgreSQL connection string is stored as an SSM SecureString at `/sentrics-core/{env}/database-url` and resolved at Lambda cold start. The MongoDB URL for `headend-api` follows the same pattern at `/ensure-cloud/headend-api/events-mongo-url`.

IAM policies grant `ssm:GetParameter` scoped to the exact parameter path, plus `kms:Decrypt` for the default SSM KMS key.
