#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

QUEUE_NAME="core-change-events-queue"
LAMBDA_URL="$CORE_CHANGE_PUBLISHER_INVOKE_URL_LOCAL"
BRIDGE_ENDPOINT_URL="${BRIDGE_AWS_ENDPOINT_URL:-${AWS_ENDPOINT_URL:-}}"

if [[ -z "$BRIDGE_ENDPOINT_URL" ]]; then
    echo "Error: AWS_ENDPOINT_URL must be set for local bridge"
    exit 1
fi

aws_local() {
    aws --region "${AWS_REGION:-us-east-1}" --endpoint-url "$BRIDGE_ENDPOINT_URL" "$@"
}

queue_url=$(aws_local sqs get-queue-url --queue-name "$QUEUE_NAME" --query QueueUrl --output text)

if [[ -z "$queue_url" ]]; then
    echo "Error: failed to resolve queue URL for $QUEUE_NAME"
    exit 1
fi

echo "Bridge polling $queue_url and forwarding to $LAMBDA_URL"

while true; do
    resp=$(aws_local sqs receive-message \
        --queue-url "$queue_url" \
        --max-number-of-messages 10 \
        --wait-time-seconds 1 \
        --output json 2>/dev/null || true)

    if [[ -z "$resp" ]]; then
        sleep 1
        continue
    fi

    payload_b64=$(python3 - <<'PY' "$resp"
import base64
import json
import sys

try:
    resp = json.loads(sys.argv[1])
except json.JSONDecodeError:
    sys.exit(0)
messages = resp.get("Messages") or []

if not messages:
    sys.exit(0)

payload = {"Records": []}
for msg in messages:
    payload["Records"].append({
        "messageId": msg.get("MessageId"),
        "body": msg.get("Body"),
    })

print(base64.b64encode(json.dumps(payload).encode()).decode())
PY
)

    if [[ -z "$payload_b64" ]]; then
        continue
    fi

    payload=$(printf "%s" "$payload_b64" | base64 --decode)

    status=$(curl -s -o /tmp/core-change-publisher.bridge.response \
        -w "%{http_code}" \
        -X POST "$LAMBDA_URL" \
        -H "Content-Type: application/json" \
        -d "$payload" || true)

    if [[ "$status" == "200" ]]; then
        receipts=$(python3 - <<'PY' "$resp" "/tmp/core-change-publisher.bridge.response"
import json
import sys

resp = json.loads(sys.argv[1])
messages = resp.get("Messages") or []

failed_ids = set()
try:
    with open(sys.argv[2], "r", encoding="utf-8") as handle:
        body = handle.read().strip()
        if body:
            payload = json.loads(body)
            failures = payload.get("batchItemFailures") or []
            for entry in failures:
                item_id = entry.get("itemIdentifier")
                if item_id:
                    failed_ids.add(item_id)
except (OSError, json.JSONDecodeError):
    failed_ids = set()

for msg in messages:
    message_id = msg.get("MessageId")
    receipt = msg.get("ReceiptHandle") or ""
    if not receipt:
        continue
    if message_id and message_id in failed_ids:
        continue
    print(receipt)
PY
)
        while read -r receipt; do
            if [[ -n "$receipt" ]]; then
                aws_local sqs delete-message --queue-url "$queue_url" --receipt-handle "$receipt" >/dev/null
            fi
        done <<< "$receipts"
    else
        echo "Bridge invoke failed (status $status). Leaving message in queue."
    fi

done
