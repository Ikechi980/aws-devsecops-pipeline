# Core Change Publisher

An AWS Lambda that consumes change events from sentrics-core resources and forwards relevant events to Ensure communities through the headend-gateway SNS topic.

## What It Does

- Receives SNS change event messages from sentrics-core via an SQS trigger
- Looks up the Ensure community mapped to the core community ID
- Publishes the original change event payload to the headend-gateway SNS topic with a message type and version

If no mapping exists, the event is skipped.

## Headend Message Format

Messages sent to the headend-gateway include:
- `target_community_id` (string, lowercase ensure community ID)
- `message_type` (string)
- `versions` (array of `version` + `payload`)

Each version is a simple integer. Services can include multiple versions to accommodate headends running different software versions.

## Local Development

Shared development infrastructure is managed at the repository root and includes LocalStack and the mock systems API.

Run the Lambda locally with the runtime emulator:
```bash
cd core-change-publisher
./scripts/dev.sh run
```

The dev script starts a lightweight bridge that pulls messages from the LocalStack `core-change-events-queue` and invokes the local Lambda runtime when running `./scripts/dev.sh run`. Configuration is read from `.env` (created from `.env.example` on first run).
For cross-repo integration, the bridge can consume from a different endpoint/queue using `BRIDGE_AWS_ENDPOINT_URL` and `BRIDGE_QUEUE_NAME`.

### Publishing a Test Event

Publish a change event to the mock core change topic:
```bash
awslocal sns publish \
  --topic-arn arn:aws:sns:us-east-1:000000000000:core-change-events \
  --message '{"event_id":"00000000-0000-0000-0000-000000000000","resource_type":"community","timestamp":"2024-01-15T10:30:00Z","event_type":"update","requester":{"type":"service","role_name":"local-dev"},"after":{"id":"11111111-1111-1111-1111-111111111111","name":"Alpha"},"before":{"id":"11111111-1111-1111-1111-111111111111","name":"Alpha"}}'
```

If the core community ID is mapped in the mock systems API, the message will be published to the `headend-messages` topic.

## Configuration

All configuration is via environment variables. See `.env.example` for the full list.

| Variable | Description |
|----------|-------------|
| `SYSTEMS_API_BASE_URL` | Base URL for the Ensure systems API (e.g., `http://localhost:8081`) |
| `HEADEND_SNS_TOPIC_ARN` | SNS topic ARN for headend-gateway messages |
| `AWS_ENDPOINT_URL` | LocalStack endpoint URL (optional, local only) |
| `BRIDGE_AWS_ENDPOINT_URL` | Optional LocalStack endpoint for bridge consumption only |
| `BRIDGE_QUEUE_NAME` | Optional queue name for bridge consumption (defaults to `core-change-events-queue`) |
| `RUST_LOG` | Log level (optional) |

## Deployment Notes

This service runs as an AWS Lambda using the Rust custom runtime.

**Runtime and artifact**
- Build with `cargo lambda build --release --output-format zip --bin core-change-publisher`.
- Deploy `target/lambda/core-change-publisher/bootstrap.zip`.
- Configure the Lambda runtime in infrastructure as an OS-only runtime such as `provided.al2023`.

**AWS resources and wiring**
- SNS topic (sentrics-core change events) -> SQS queue -> Lambda trigger.
- SNS topic for headend-gateway messages (this service publishes).

**IAM**
- `sns:Publish` on the headend-gateway topic.
- `sqs:ReceiveMessage`, `sqs:DeleteMessage`, `sqs:GetQueueAttributes` on the trigger queue.
- Standard Lambda logging permissions.

**Networking**
- Outbound HTTPS access to the Ensure systems API.

**Runtime configuration**
- `SYSTEMS_API_BASE_URL`, `HEADEND_SNS_TOPIC_ARN`, `AWS_REGION`.
- `RUST_LOG` (optional).
