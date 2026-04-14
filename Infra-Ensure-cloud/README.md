# infra-sentrics-core Terraform Infrastructure

Terraform code for provisioning the AWS infrastructure that runs the sentrics-core API stack.

This repository is **infrastructure only**. The application build and Lambda artifact creation happen in a separate repo.  
---

## Scope

This repo manages:

- AWS Lambda (Rust, ZIP-based deployment)
  - Main API function
  - Database migration function
- API Gateway HTTP APIs
  - Public API with `$default` route
  - Azure AD JWT authorizer
  - CORS configuration
  - Internal API secured with AWS IAM
- Amazon RDS PostgreSQL (private subnets)
- SNS topic + email subscriptions
- IAM roles and permissions
- VPC and subnet resolution via data sources
- Environment-specific configuration via `*.tfvars`
---

## Architecture Overview

- **API Gateway**
  - Public HTTP API with `$default` route
  - Internal HTTP API with AWS IAM auth
  - All routing handled inside Lambda

- **Authentication**
  - Azure AD JWT authorizer at API Gateway (public API)
  - AWS IAM auth for internal API
  - Lambda assumes requests are already authenticated

- **Lambda Deployment**
  - Built as ZIP artifacts in the build repo
  - Stored in S3 at stable keys per environment
  - S3 versioning enabled for rollback

- **Database**
  - PostgreSQL on RDS
  - Private subnets only
  - Credentials are not hardcoded (password is generated at apply time)

- **Messaging**
  - SNS topic for API events
  - VPC endpoint for private SNS access

---

## Environments

Each environment is driven by its own `.tfvars` file:

- `dev.tfvars`
- `prod.tfvars`

These files define:
- Environment name
- Region
- JWT configuration
- Lambda artifact location
- RDS sizing and behavior
- SNS topic + subscribers

AWS resource IDs are resolved dynamically via data sources.

---

## Lambda Artifact Strategy

- Lambda is deployed from S3 ZIP artifacts produced by the build repo
- Artifact keys are stable per environment
- Each build overwrites the object
- S3 versioning preserves previous versions
- Terraform always points to the same key

---

## Authentication Contract

The public API requires:

- Azure AD **access tokens**
- Correct issuer and audience
- Requests authenticated before reaching Lambda

The internal API requires:

- AWS IAM-signed requests

---

## Deployment

AWS CodeBuild runs Terraform:

- buildspec.yaml

Environment variables used:
- `ENVIRONMENT`
- `TERRAFORM_INPUT`
- `BUCKET_NAME`

---

## Repository Structure
```text
.
|-- .gitignore
|-- README.md
|-- buildspec.yaml
`-- iac
    |-- .terraform.lock.hcl         # Provider dependency lock file
    |-- backend.tf                  # Terraform backend configuration
    |-- buildspec-lambda-zips.yaml  # Buildspec for Lambda ZIP packaging
    |-- codepipeline.tf             # CodePipeline definition (source/build/deploy)
    |-- data.tf                     # Data sources
    |-- dev-backend.tfvars          # Dev backend settings
    |-- dev.tfvars                  # Dev environment values
    |-- locals.tf                   # Local values and derived maps
    |-- main.tf                     # Core infrastructure resources/modules
    |-- provider.tf                 # AWS provider configuration
    |-- variables.tf                # Input variable declarations
    |-- versions.tf                 # Terraform/provider version constraints
    `-- taskdefs
        |-- headend-gateway.json    # ECS task definition template
        |-- pki-api.json            # ECS task definition template
        `-- stepca.json             # ECS task definition template
```
