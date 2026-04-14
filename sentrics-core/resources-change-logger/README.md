# Resources Change Logger

An AWS Lambda that consumes change events from resources-api via SQS and stores them in DynamoDB for audit logging.

## What It Does

- Receives SNS change event messages through an SQS trigger
- Normalizes requester and resource identifiers
- Writes event payloads to DynamoDB with community, resource, and requester indexes
- Ignores duplicate events using a conditional write on `event_id`

## DynamoDB Schema

The table uses the following key design:

- **community_pk**: `COMMUNITY#<community_id>`
- **timestamp_sk**: `TS#<timestamp>#<event_id>`
- **GSI `by_resource` PK (`resource_pk`)**: `RESOURCE#<resource_type>#<resource_id>`
- **GSI `by_resource` SK (`timestamp_sk`)**: `TS#<timestamp>#<event_id>`
- **GSI `by_requester` PK (`requester_pk`)**: `REQUESTER#<requester_type>#<requester_id>`
- **GSI `by_requester` SK (`timestamp_sk`)**: `TS#<timestamp>#<event_id>`

Stored attributes include `event_id`, `timestamp`, `resource_type`, `resource_id`, `community_id`, `event_type`, `requester_type`, `requester_id`, `requester`, `before`, and `after`.

## Local Development

Shared development infrastructure is managed at the repository root and includes LocalStack.

Run the Lambda locally with the runtime emulator:
```bash
cd resources-change-logger
./scripts/dev.sh run
```

Publish a test change event:
```bash
awslocal sns publish \
  --topic-arn arn:aws:sns:us-east-1:000000000000:resources-events \
  --message '{"event_id":"00000000-0000-0000-0000-000000000000","resource_type":"community","timestamp":"2024-01-15T10:30:00Z","event_type":"update","requester":{"type":"local_dev"},"after":{"id":"11111111-1111-1111-1111-111111111111","name":"Alpha"},"before":{"id":"11111111-1111-1111-1111-111111111111","name":"Alpha"}}'
```

## Configuration

All configuration is via environment variables. See `.env.example` for the full list.

| Variable | Description |
|----------|-------------|
| `CHANGE_LOG_TABLE_NAME` | DynamoDB table name for change log entries |
| `AWS_ENDPOINT_URL` | LocalStack endpoint URL (optional, local only) |
| `AWS_REGION` | AWS region |
| `RUST_LOG` | Log level (optional, defaults to `info`) |

## Deployment

Build the Lambda binary for deployment:
```bash
cargo lambda build --release --output-format zip --bin resources-change-logger
```

This produces:
- `target/lambda/resources-change-logger/bootstrap.zip`

### Lambda

The resources-change-logger binary requires these environment variables:
- `CHANGE_LOG_TABLE_NAME`
- `AWS_REGION`
- `RUST_LOG` (optional, defaults to `info`)

Configure the Lambda with an SQS trigger subscribed to the resources-api SNS topic and grant permissions for `dynamodb:PutItem` on the change log table and `sqs:ReceiveMessage/DeleteMessage` for the trigger queue.

### Infrastructure Notes

**DynamoDB**
- Table name is provided via `CHANGE_LOG_TABLE_NAME`.
- Primary key: `community_pk` (string) + `timestamp_sk` (string).
- GSI `by_resource`: keys `resource_pk` (string) + `timestamp_sk` (string).
- GSI `by_requester`: keys `requester_pk` (string) + `timestamp_sk` (string).

**SQS / SNS**
- Standard queue is sufficient; FIFO is not required.
- The SNS subscription relies on a queue policy that allows the topic to publish.
- The Lambda trigger supports partial batch response (batch item failures).
- Typical deployments use a visibility timeout longer than max Lambda runtime.
- Typical deployments include a DLQ with a redrive policy for poison messages.
- Typical deployments set message retention long enough to cover outages or deployments.

## Deployment Notes

This section summarizes deployment requirements and constraints for AWS.

### Deployment Unit

- One AWS Lambda function (`resources-change-logger`) triggered by an SQS queue.
- The SQS queue is subscribed to the resources-api SNS change events topic.

### Required AWS Resources

- SNS topic used by resources-api for change events.
- SQS standard queue subscribed to that topic.
- DynamoDB table with the primary key and GSIs described above.

### Required Environment Variables

- `CHANGE_LOG_TABLE_NAME`
- `AWS_REGION`
- `RUST_LOG` (optional)

### IAM Permissions

- `dynamodb:PutItem` on the change log table.
- `sqs:ReceiveMessage`, `sqs:DeleteMessage`, `sqs:GetQueueAttributes` on the trigger queue.

### Constraints and Expectations

- Messages arrive in the SNS-to-SQS envelope format (no raw message delivery).
- Event processing is idempotent; duplicate `event_id` values are ignored via conditional write.
- Ordering is not required; a standard queue is sufficient.
