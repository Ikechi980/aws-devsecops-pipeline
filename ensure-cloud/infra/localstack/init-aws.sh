#!/bin/bash
set -e

echo "Initializing LocalStack SQS/SNS resources..."

MONGO_URL_PARAMETER_NAME="/ensure-cloud/headend-api/events-mongo-url"
LOCAL_MONGO_URL="mongodb://developer:developer@localhost:27017/global-events?authSource=global-events"

awslocal ssm put-parameter \
  --name "$MONGO_URL_PARAMETER_NAME" \
  --type SecureString \
  --value "$LOCAL_MONGO_URL" \
  --overwrite

# Create SNS topic for message broadcasting
awslocal sns create-topic --name headend-messages

# Create SNS topic for core change events
awslocal sns create-topic --name core-change-events

# Create a test SQS queue for integration testing (to verify published messages)
awslocal sqs create-queue --queue-name headend-test-queue

# Create SQS queue for core change events
awslocal sqs create-queue --queue-name core-change-events-queue

# Subscribe test queue to topic for easier testing
TOPIC_ARN="arn:aws:sns:us-east-1:000000000000:headend-messages"
QUEUE_ARN="arn:aws:sqs:us-east-1:000000000000:headend-test-queue"
awslocal sns subscribe --topic-arn "$TOPIC_ARN" --protocol sqs --notification-endpoint "$QUEUE_ARN"

CORE_TOPIC_ARN="arn:aws:sns:us-east-1:000000000000:core-change-events"
CORE_QUEUE_ARN="arn:aws:sqs:us-east-1:000000000000:core-change-events-queue"
awslocal sns subscribe --topic-arn "$CORE_TOPIC_ARN" --protocol sqs --notification-endpoint "$CORE_QUEUE_ARN"

echo "LocalStack initialization complete!"
echo "Mongo URL parameter: $MONGO_URL_PARAMETER_NAME"
echo "Topic ARN: $TOPIC_ARN"
echo "Test Queue ARN: $QUEUE_ARN"
echo "Core Change Topic ARN: $CORE_TOPIC_ARN"
echo "Core Change Queue ARN: $CORE_QUEUE_ARN"
