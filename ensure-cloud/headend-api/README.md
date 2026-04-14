# Headend API

An AWS Lambda that serves core community resources and events to deployed headends. Requests arrive through API Gateway with mutual TLS (mTLS). The service extracts the Common Name (CN) from the client certificate, maps the Ensure community ID to the sentrics-core community ID via ensure360-ems for core resources, and queries MongoDB for events data.

## What It Does

- Accepts mTLS-authenticated requests from headends
- Extracts the Ensure community ID from the client cert CN (`<community-id>.ensurelink.net`)
- For core endpoints: looks up the core community ID via ensure360-ems and returns data from sentrics-core (requests are signed with AWS SigV4)
- For events endpoint: queries MongoDB for community events filtered by payload type and date range

If a community has no core mapping, the API returns 404 for core endpoints.

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/health` | GET | Health check |
| `/v1/core/community` | GET | Core community details for the calling headend |
| `/v1/core/locations` | GET | Core locations for the calling headend |
| `/v1/core/residents` | GET | Core residents for the calling headend |
| `/v1/core/residents/:id/photo` | GET | Resident photo bytes for the calling headend |
| `/v1/events` | GET | Events for the calling headend (query params: `payloadTypes`, `afterDate`, `beforeDate`, `limit`) |

## Local Development

Shared development infrastructure is managed at the repository root and includes the mock systems API and the mock core resources API.

Run the Lambda locally:
```bash
cd headend-api
./scripts/dev.sh run
```

The application reads configuration from `.env` (created from `.env.example` on first run). Local development resolves the Mongo connection string from LocalStack SSM using the parameter name in `EVENTS_MONGO_URL_SSM_PARAMETER`. For local development without API Gateway context, enable `ALLOW_UNAUTHENTICATED` and pass `x-ensure-community-id` in the request headers. The Lambda URL is available at `http://localhost:9202/lambda-url/headend-api`.

When shared nginx is running, you can also test a local mTLS front door at `https://localhost:8443/api/v1/*`.

## Configuration

All configuration is via environment variables. See `.env.example` for the full list.

| Variable | Description |
|----------|-------------|
| `SYSTEMS_API_BASE_URL` | Base URL for ensure360-ems (e.g., `http://localhost:8081`) |
| `CORE_RESOURCES_API_BASE_URL` | Base URL for sentrics-core resources API (e.g., `http://localhost:8082`) |
| `EVENTS_MONGO_URL_SSM_PARAMETER` | SSM parameter name containing the MongoDB connection URL (e.g., `/ensure-cloud/headend-api/events-mongo-url`) |
| `EVENTS_LIMIT_DEFAULT` | Default limit for events queries (e.g., `100`) |
| `EVENTS_LIMIT_MAX` | Maximum limit for events queries (e.g., `1000`) |
| `AWS_REGION` | AWS region for SigV4 signing (e.g., `us-east-1`) |
| `AWS_ACCESS_KEY_ID` | AWS access key for SigV4 signing |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key for SigV4 signing |
| `ALLOW_UNAUTHENTICATED` | Enables local development fallback (optional, local only) |
| `RUST_LOG` | Log level (optional) |

## Error Responses

Error responses are documented in `ERRORS.md`.

## Deployment Notes

This service runs as an AWS Lambda using the Rust custom runtime behind API Gateway.

**Runtime and artifact**
- Build with `cargo lambda build --release --output-format zip --bin headend-api`.
- Deploy `target/lambda/headend-api/bootstrap.zip`.
- Configure the Lambda runtime in infrastructure as an OS-only runtime such as `provided.al2023`.

**AWS resources and wiring**
- API Gateway HTTP API (payload format v2.0) -> Lambda integration.
- API Gateway must enforce mTLS and include client cert details in the request context.
- The API Gateway trust store must use the same CA material as step-ca (PKI service).

**IAM**
- Standard Lambda logging permissions.
- `ssm:GetParameter` for the configured Mongo URL parameter.
- If the parameter uses a customer-managed KMS key, decrypt permission for that key.

**Networking**
- Outbound HTTPS access to ensure360-ems and the sentrics-core resources API.
- Outbound access to MongoDB.

**Runtime configuration**
- `SYSTEMS_API_BASE_URL`, `CORE_RESOURCES_API_BASE_URL`.
- `EVENTS_MONGO_URL_SSM_PARAMETER` (SSM parameter containing the MongoDB connection string with auth).
- Events limits: `EVENTS_LIMIT_DEFAULT`, `EVENTS_LIMIT_MAX`.
- AWS credentials from Lambda execution role (automatic).
- `RUST_LOG` (optional).

**Constraints**
- The Lambda reads `requestContext.authentication.clientCert.subjectDN` and extracts the CN.
- The CN must be formatted as `<community-id>.ensurelink.net`; missing client cert details result in 500.
