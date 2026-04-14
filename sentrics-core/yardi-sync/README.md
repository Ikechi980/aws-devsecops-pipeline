# Yardi Sync

A background service that synchronizes resident and location data from Yardi EHR to the Sentrics resources-api. It monitors for configuration changes via SNS/SQS events and polls the Yardi FHIR API at a configurable interval to detect and apply changes.

For a deeper understanding of how the service works and guarantees eventual consistency, see [ARCHITECTURE.md](./ARCHITECTURE.md).

## How It Works

This service maintains synchronization between Yardi EHR and resources-api for communities that have Yardi integrations configured. When a community is set up with Yardi credentials in resources-api, this service automatically:

1. Detects the new community via change events from resources-api
2. Polls the Yardi FHIR API to fetch locations and residents (rooms only)
3. Compares the Yardi data with the current resources-api state
4. Creates, updates, or deletes resources-api records to match Yardi
5. Publishes failure notifications when Yardi API issues are detected

The sync is one-directional: Yardi is the source of truth, and resources-api is kept in sync with it.

### Sync Behavior

**Locations**: Only Yardi room locations are created in resources-api, name changes are propagated, and rooms that no longer appear in Yardi are deleted from resources-api.

**Residents**: New Yardi residents are created in resources-api, first/last name and room changes are propagated, and residents that no longer appear in Yardi are deleted from resources-api. A resident must resolve to a room location in resources-api before they can be created; otherwise the sync records a data invariant violation.

The sync applies changes in a specific order to maintain referential integrity: first delete residents, then delete locations, then create/update locations, and finally create/update residents.

**Performance Optimization**: The service uses a "dirty flag" approach to minimize load on resources-api. When change events are received for a community, the community is marked dirty. On each poll cycle, the service only re-fetches resources-api data for communities that are dirty or haven't been refreshed within the configured interval. All communities have their Yardi data fetched on every poll since Yardi doesn't provide change notifications.

**Race Condition Handling**: If the service attempts to update or delete a resource that has already been deleted (404 response), it marks the community dirty to trigger a fresh fetch on the next poll cycle, ensuring eventual consistency.

### Failure Handling

When the service encounters issues with the Yardi API, it publishes notifications to the configured SNS topic. Failures are deduplicated so that each unique failure is only published once until it is resolved. Once a successful sync occurs, the failure is cleared and will be re-published if it recurs.

| Failure Type | Description |
|--------------|-------------|
| `yardi_unreachable` | The Yardi API endpoint is not responding |
| `yardi_credentials_invalid` | The community's Yardi credentials were rejected |
| `yardi_data_invariant_violation` | Unexpected data structure from Yardi (e.g., patient without encounter) |
| `yardi_unexpected_response` | Yardi returned an error or unparseable response |

## Configuration

All configuration is via environment variables. See `.env.example` for the full list.

| Variable | Description |
|----------|-------------|
| `RESOURCES_API_BASE_URL` | Base URL of the resources-api (e.g., `http://127.0.0.1:9000/lambda-url/resources-api/v1`) |
| `YARDI_POLL_INTERVAL_MS` | How often to poll Yardi for changes (e.g., `10000`) |
| `RESOURCES_REFRESH_INTERVAL_SECS` | Maximum time between resources-api refreshes, even if no changes detected (e.g., `300` for 5 minutes) |
| `RESOURCES_EVENTS_QUEUE_URL` | SQS queue URL for receiving resources-api change events |
| `FAILURE_SNS_TOPIC_ARN` | SNS topic ARN for publishing failure notifications |
| `AWS_REGION` | AWS region used for signing requests to resources-api and for AWS service clients |
| `AWS_ENDPOINT_URL` | LocalStack endpoint (optional, for local development) |
| `RUST_LOG` | Log level (optional, defaults to `info`) |

All requests to resources-api are signed with IAM SigV4 using the configured AWS region and credentials.

## Local Development

The local development environment uses shared infrastructure managed at the repository root. This includes PostgreSQL (used by resources-api) and LocalStack (providing SNS and SQS services).

### Getting Started

Start the full local environment with a single command. The script brings up infrastructure, starts resources-api, and runs yardi-sync. It also shuts everything down on Ctrl-C.

```bash
cd /path/to/sentrics-core/yardi-sync
./scripts/dev.sh run
```

### Configuration

Local configuration is loaded from the `.env` file. The first time you run `./scripts/dev.sh run`, it will create this file from `.env.example`. You will need to update the Yardi credentials in this file to point at your Yardi sandbox or production instance.

Shared infrastructure values (database, LocalStack, SNS/SQS ARNs) are defined in `../infra/dev.env` and are automatically loaded by the dev scripts. You can override these in your local `.env` file to point at staging or production services.

### Testing Against Real Services

You can test integration with real external services while keeping other dependencies local by
creating a Yardi-enabled community in `resources-api` with that tenant's
`yardi_api_base_url` and `yardi_token_url`:

```bash
RESOURCES_API_BASE_URL=https://staging-api.example.com/v1
```

This allows you to validate integration with the Yardi sandbox while using your local resources-api for development.

### Testing

Run tests with:

```bash
cd /path/to/sentrics-core/yardi-sync
./scripts/dev.sh test
```

Tests start the shared development environment (including mock-yardi-api) and resources-api before running.

For full sync behavior testing, also run yardi-sync:

```bash
cargo run
```

For detailed test coverage information, see [TEST_COVERAGE.md](./TEST_COVERAGE.md).

## SQS Queue Requirements

The SQS queue that receives resources-api change events should be configured as a **standard queue** (not FIFO). The polling mechanism provides eventual consistency, so strict message ordering is not required.

Configuration recommendations:

- **Message retention period**: At least 4 days (345600 seconds) to handle temporary service outages
- **Visibility timeout**: 30 seconds or longer to allow message processing
- **Dead-letter queue**: Recommended for messages that repeatedly fail processing

The queue must be subscribed to the resources-api SNS topic. No filter policy is needed since this service processes all resource types (communities, locations, and residents).

## Deployment

### Building the Docker Image

```bash
docker build -f infra/yardi-sync/Dockerfile -t yardi-sync:latest .
```

### ECS Task Configuration

The ECS task requires the following IAM permissions:

- `sqs:ReceiveMessage`, `sqs:DeleteMessage`, `sqs:GetQueueAttributes` on the events queue
- `sns:Publish` on the failure notifications topic
- `execute-api:Invoke` on the resources-api HTTP API

The task should be configured with:

- Sufficient memory for the Rust runtime (512MB recommended minimum)
- Health checks are not required since this is a background worker, not an HTTP service
- Graceful shutdown handling (the service responds to SIGTERM)

Ensure all required environment variables are set. The service will fail fast on startup if any required configuration is missing.

## Deployment Notes

This section summarizes deployment requirements and constraints for AWS.

### Deployment Unit

- Long-running container service (ECS task or equivalent); no inbound HTTP listener.

### Required AWS Resources

- SQS standard queue subscribed to the resources-api SNS change events topic.
- SNS topic for sync failure notifications (`FAILURE_SNS_TOPIC_ARN`).
- Network egress to the Yardi FHIR API and the resources-api base URL.

### Required Environment Variables

- `RESOURCES_API_BASE_URL`
- `YARDI_POLL_INTERVAL_MS`
- `RESOURCES_REFRESH_INTERVAL_SECS`
- `RESOURCES_EVENTS_QUEUE_URL`
- `FAILURE_SNS_TOPIC_ARN`
- `AWS_REGION`
- `RUST_LOG` (optional)

### IAM Permissions

- `sqs:ReceiveMessage`, `sqs:DeleteMessage`, `sqs:GetQueueAttributes` on the events queue.
- `sns:Publish` on the failure notifications topic.
- `execute-api:Invoke` on the resources-api HTTP API.

### Constraints and Expectations

- Yardi is the source of truth; the service applies one-way sync into resources-api.
- The process is polling-based; ordering is not required and a standard SQS queue is sufficient.
- The service handles SIGTERM for graceful shutdown and does not expose a health endpoint.
