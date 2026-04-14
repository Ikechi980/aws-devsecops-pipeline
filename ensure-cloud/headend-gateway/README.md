# Headend Gateway

A WebSocket gateway service that enables real-time communication between cloud services and deployed headend devices. The gateway accepts persistent WebSocket connections from devices authenticated via mutual TLS (mTLS) and routes messages from cloud services to specific connected devices using an SNS/SQS fan-out pattern.

## Architecture

The gateway operates in AWS ECS behind an Application Load Balancer (ALB) configured for mTLS client authentication. When a device establishes a WebSocket connection, the ALB validates the client certificate and forwards the connection to the gateway with the client's Distinguished Name in an HTTP header. The gateway extracts the Common Name (CN) from this header to identify the device.

Cloud services publish messages to an SNS topic with the target `community_id`. Each running gateway instance maintains an ephemeral SQS queue subscribed to this topic. When a message arrives, the gateway checks if the target device is connected to that instance. If so, it forwards the message over the WebSocket connection. If not, the message is discarded (another instance will handle it, or the device is offline).

This fan-out pattern eliminates the need for sticky sessions or centralized state management (like Redis). Each gateway instance operates independently, handling only its directly connected devices.

## Local Development

Shared development infrastructure is managed at the repository root and includes Step CA, mock-systems-api, LocalStack, and nginx (mTLS termination).

### Getting Started

Run headend-gateway with its dependency (pki):
```bash
cd headend-gateway
./scripts/dev.sh run
```

This starts shared infrastructure, runs pki, and then runs headend-gateway. The application reads configuration from `.env` (created from `.env.example` on first run).

### Development Scripts

- `./scripts/dev.sh deps` - Start shared infra and pki, then tail logs
- `./scripts/dev.sh run` - Start infra and pki, then run headend-gateway
- `./scripts/dev.sh test` - Start infra and pki, run headend-gateway, then run tests

### Testing with wscat

To manually connect, request a client certificate from pki and use it with wscat. The nginx trust store uses the dev Step CA root certificate.

To send a test message from LocalStack SNS:

```bash
awslocal sns publish \
  --topic-arn arn:aws:sns:us-east-1:000000000000:headend-messages \
  --message '{"target_community_id":"testdevice01","message_type":"core_change_event","versions":[{"version":1,"payload":{"message":"Hello!"}}]}'
```

## Configuration

The application is configured entirely through environment variables. All variables are required unless noted.

| Variable | Description |
|----------|-------------|
| `HOST` | IP address to bind to (e.g., `0.0.0.0`) |
| `PORT` | Port to listen on (e.g., `3000`) |
| `HEADEND_SNS_TOPIC_ARN` | ARN of the SNS topic for broadcasting messages |
| `AWS_ENDPOINT_URL` | LocalStack endpoint URL (optional, for local development only) |
| `RUST_LOG` | Log level (optional, defaults to application defaults) |

## Message Format

Services should publish messages to SNS with this JSON structure:

```json
{
  "target_community_id": "device-identifier",
  "message_type": "core_change_event",
  "versions": [
    { "version": 1, "payload": { "example": "message payload" } }
  ]
}
```

For multiple versions, add more entries to `versions`:

```json
{
  "target_community_id": "device-identifier",
  "message_type": "core_change_event",
  "versions": [
    { "version": 1, "payload": { "example": "payload v1" } },
    { "version": 2, "payload": { "example": "payload v2" } }
  ]
}
```

The `target_community_id` must be a lowercase slug (`[a-z0-9-]+`) that matches the normalized community ID derived from the device certificate CN (`<community-id>.ensurelink.net`). The gateway forwards the message payload to the device over the WebSocket connection.

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/health` | GET | Returns health status and current connected client count |
| `/v1/ws` | GET | WebSocket upgrade endpoint (requires mTLS) |

## Building for Production

Build the Docker image:

```bash
docker build -f infra/headend-gateway/Dockerfile -t headend-gateway:latest .
```

The image uses a multi-stage build with Rust compilation in the first stage and a minimal Debian runtime in the second stage. The final image runs as a non-root user.

## Deployment Notes

This service runs as an ECS service behind an ALB and accepts WebSocket connections.

**Runtime and artifact**
- Build the image from `infra/headend-gateway/Dockerfile`.
- Deploy it as the `headend-gateway` container.

**AWS resources and wiring**
- ALB (mTLS termination) -> ECS service (WebSocket upgrade on `/v1/ws`).
- SNS topic for headend messages (this service subscribes).
- Each task creates an ephemeral SQS queue and SNS subscription on startup.

**IAM**
- `sqs:CreateQueue`, `sqs:DeleteQueue`, `sqs:ReceiveMessage`, `sqs:DeleteMessage`, `sqs:GetQueueAttributes`, `sqs:SetQueueAttributes`.
- `sns:Subscribe`, `sns:Unsubscribe`.
- Standard ECS task logging permissions.

**Networking**
- Service should be reachable only from the ALB.
- Outbound access to SNS/SQS.

**Runtime configuration**
- `HOST`, `PORT`, `HEADEND_SNS_TOPIC_ARN`.
- `RUST_LOG` (optional).

**Constraints**
- The ALB must inject the client Subject DN header, and the gateway reads `X-Amzn-Mtls-Clientcert-Subject`.
- The ALB trust store must use the same CA material as step-ca (PKI service).
- The CN in the client certificate must be `<community-id>.ensurelink.net`; the gateway normalizes this to lowercase `community_id` for routing.
