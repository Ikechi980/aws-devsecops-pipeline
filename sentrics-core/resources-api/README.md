# Resources API

A REST API for managing communities, locations, and residents, built with Rust and deployed as an AWS Lambda function behind API Gateway.

## Quick Start

Install the necessary tools and start local development:
```bash
cargo install cargo-lambda --locked --version 1.9.1
cargo install sqlx-cli --no-default-features --features rustls,postgres

# Start resources-api with infrastructure and migrations
cd /path/to/sentrics-core/resources-api
./scripts/dev.sh run
```

The API runs locally on port 9000 with automatic reload on code changes. You can test it immediately with `curl http://127.0.0.1:9000/lambda-url/resources-api/v1/health`. The dev script manages infrastructure and shuts everything down on Ctrl-C.

This project requires `cargo-lambda 1.9.1` locally. Older versions are not supported by the development scripts.

Local configuration is loaded from the `.env` file, which you can customize to override shared settings or point at remote services. See `.env.example` for the configuration template and `../infra/dev.env` for shared infrastructure values.

## API Endpoints

All endpoints are prefixed with `/v1` and follow RESTful conventions:

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET/POST | `/communities` | List or create communities |
| GET/PUT/DELETE | `/communities/:id` | Get, update, or delete a community |
| GET/POST | `/communities/:id/locations` | List or create locations |
| GET/PUT/DELETE | `/communities/:id/locations/:lid` | Get, update, or delete a location |
| GET/POST | `/communities/:id/residents` | List or create residents |
| GET/PUT/DELETE | `/communities/:id/residents/:rid` | Get, update, or delete a resident |

Resources are hierarchical, with communities at the top, locations nested within communities, and residents nested within both. Communities and locations require a non-empty `name`. Residents require a non-empty `last_name`, and `first_name` may be an empty string. Attempting to delete a resource with dependent children returns a 409 conflict error.

## Authentication

This API is designed to run behind AWS API Gateway with two authentication methods:

- **Microsoft Entra ID (Azure AD)** for end-user authentication
- **IAM SigV4** for service-to-service communication

All mutating endpoints (POST, PUT, DELETE) require authentication and extract the requester's identity from the API Gateway request context. This identity is included in all change events published to SNS.

For Entra ID-authenticated requests, the API extracts the user's `preferred_username` and optional `name` claim from the JWT. For IAM-authenticated requests, the API extracts the role name from the caller's ARN.

Requests without valid authentication credentials receive a 401 Unauthorized response.

### Change Event Format

When a resource is created, updated, or deleted, the API publishes an event to SNS with the following structure. The `event_id` is a UUID generated at publication time.

```json
{
  "event_id": "b3a0f1e6-4a5c-4dd7-9b6b-3f0cfe1b6a2d",
  "resource_type": "community",
  "timestamp": "2024-01-15T10:30:00Z",
  "event_type": "create",
  "requester": {
    "type": "user",
    "username": "john.doe@example.com",
    "name": "John Doe"
  },
  "after": { "id": "...", "first_name": "...", "last_name": "..." }
}
```

For service-to-service requests, the requester field looks like:

```json
{
  "type": "service",
  "role_name": "DataSyncServiceRole"
}
```

### Local Development

When running locally with `cargo lambda watch`, there is no API Gateway context, so authentication information is not available. For local development and testing, the `.env` file includes `ALLOW_UNAUTHENTICATED=1`, which enables a fallback identity of `{"type": "service", "role_name": "local-dev"}` for all requests.

**Warning**: Never set `ALLOW_UNAUTHENTICATED` in production environments. Remove it from environment variables before deploying to AWS.

## Testing

Run tests against a fully started local environment:
```bash
cd /path/to/sentrics-core/resources-api
./scripts/dev.sh test
```

The tests exercise the entire API, including SNS event publishing, by making real HTTP requests to the locally-running Lambda instance and verifying that events are published to LocalStack SNS.

Tests verify both happy paths and error cases, including invalid UUIDs, missing fields, empty names, cross-community access violations, and foreign key constraints. Each test creates unique data and cleans up afterward to prevent pollution. 

## Database Migrations

This project uses SQLx with compile-time query verification. The compiler checks SQL queries against your database schema at build time, which requires either a running database or offline mode with pre-generated metadata in the `.sqlx/` directory.

To add a new migration:
```bash
sqlx migrate add your_migration_name
# Edit the generated SQL file in migrations/
./scripts/migrate.sh
./scripts/prepare_offline.sh
git add .sqlx/
```

The `migrate.sh` script builds the migrate Lambda binary, starts a local Lambda runtime emulator, invokes the function, and cleans up. This exercises the same code path that runs in production.

For builds without database access, such as in CI/CD pipelines, use `SQLX_OFFLINE=true` to rely on the cached metadata instead of connecting to a database.

## Deployment

The project builds two Lambda binaries:

1. **resources-api**: The main REST API service
2. **migrate**: Runs database migrations

### Building

Build both binaries for deployment:

```bash
SQLX_OFFLINE=true cargo lambda build --release --output-format zip --compiler cargo --bin resources-api
SQLX_OFFLINE=true cargo lambda build --release --output-format zip --compiler cargo --bin migrate
```

The `--compiler cargo` flag is still required because the default cross-compiler path currently fails with SQLx in this project. This produces:
- `target/lambda/resources-api/bootstrap.zip` - The API deployment artifact
- `target/lambda/migrate/bootstrap.zip` - The migration deployment artifact

### API Lambda

The resources-api binary requires these environment variables:
- `DATABASE_URL` - PostgreSQL connection string
- `SNS_TOPIC_ARN` - ARN of the SNS topic for publishing change events
- `RUST_LOG` - Log level (optional, defaults to `info`)

Deploy behind API Gateway HTTP API (v2) with either:
- A JWT authorizer that forwards Microsoft Entra ID (Azure AD) claims, or
- IAM SigV4 authentication enabled for service-to-service requests.

Authentication is extracted from the API Gateway v2 request context, so other front doors (ALB, Lambda Function URLs, API Gateway REST APIs) are not supported.

### Migration Lambda

The migrate binary requires:
- `DATABASE_URL` - PostgreSQL connection string
- `RUST_LOG` - Log level (optional, defaults to `info`)

The migrate Lambda can be invoked directly:
```bash
aws lambda invoke --function-name migrate output.json
```

It accepts any JSON payload, even empty `{}`. On success, it returns:
```json
{"message": "Migrations completed successfully"}
```

On failure, it returns an error with details:
```json
{"errorMessage": "Failed to run migrations: ...", "errorType": "..."}
```

**Important**: Run migrations before deploying API changes that depend on schema updates. The migration Lambda is idempotent: SQLx tracks which migrations have been applied and only runs new ones.

## Deployment Notes

This section summarizes deployment requirements and constraints for AWS.

### Deployment Unit

- Two AWS Lambda functions: `resources-api` (HTTP API) and `migrate` (DB migrations).
- The API Lambda expects API Gateway HTTP API v2 (Lambda proxy) request context.

### Required AWS Resources

- PostgreSQL database reachable by the Lambda (typically in a VPC).
- SNS topic for change events (`SNS_TOPIC_ARN`).
- API Gateway HTTP API with JWT (Microsoft Entra ID) authorizer and/or IAM SigV4 auth.

### Required Environment Variables

- `DATABASE_URL`
- `SNS_TOPIC_ARN`
- `RUST_LOG` (optional)
- `ALLOW_UNAUTHENTICATED` is for local development only and must be unset in production.

### IAM Permissions

- `sns:Publish` on the change events topic.
- Network access to the PostgreSQL endpoint; if the database is in a VPC, the Lambda must be attached to appropriate subnets and security groups.

### Constraints and Expectations

- Authentication and requester identity extraction depend on API Gateway v2 request context. Other front doors are not supported.
- Run the migration Lambda before deploying API changes.
- Every create/update/delete publishes a change event; consumers should expect at-least-once delivery.
