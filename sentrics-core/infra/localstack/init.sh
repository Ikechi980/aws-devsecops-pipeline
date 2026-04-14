#!/bin/bash
set -e

echo "Initializing LocalStack for Sentrics services..."

# Create SNS topic for resources-api events
awslocal sns create-topic --name resources-events
echo "Created SNS topic: resources-events"

# Create SNS topic for yardi-sync failures
awslocal sns create-topic --name yardi-sync-failures
echo "Created SNS topic: yardi-sync-failures"

# Create SQS queue for resources-api testing
awslocal sqs create-queue --queue-name resources-events-test
echo "Created SQS queue: resources-events-test"

# Create SQS queue for yardi-sync to receive resources-api events
awslocal sqs create-queue --queue-name yardi-sync-events
echo "Created SQS queue: yardi-sync-events"

# Create SQS queue for resources-change-logger to receive resources-api events
awslocal sqs create-queue --queue-name resources-change-logger-events
echo "Created SQS queue: resources-change-logger-events"

# Create DynamoDB table for resources-change-logger
awslocal dynamodb create-table \
    --table-name resources-change-log \
    --attribute-definitions \
        AttributeName=community_pk,AttributeType=S \
        AttributeName=timestamp_sk,AttributeType=S \
        AttributeName=resource_pk,AttributeType=S \
        AttributeName=requester_pk,AttributeType=S \
    --key-schema AttributeName=community_pk,KeyType=HASH AttributeName=timestamp_sk,KeyType=RANGE \
    --global-secondary-indexes '[
        {"IndexName":"by_resource","KeySchema":[{"AttributeName":"resource_pk","KeyType":"HASH"},{"AttributeName":"timestamp_sk","KeyType":"RANGE"}],"Projection":{"ProjectionType":"ALL"}},
        {"IndexName":"by_requester","KeySchema":[{"AttributeName":"requester_pk","KeyType":"HASH"},{"AttributeName":"timestamp_sk","KeyType":"RANGE"}],"Projection":{"ProjectionType":"ALL"}}
    ]' \
    --billing-mode PAY_PER_REQUEST
echo "Created DynamoDB table: resources-change-log"

# Subscribe resources-events-test queue to resources-events topic
RESOURCES_TOPIC_ARN="arn:aws:sns:us-east-1:000000000000:resources-events"
TEST_QUEUE_ARN="arn:aws:sqs:us-east-1:000000000000:resources-events-test"
awslocal sns subscribe \
    --topic-arn "$RESOURCES_TOPIC_ARN" \
    --protocol sqs \
    --notification-endpoint "$TEST_QUEUE_ARN"
echo "Subscribed resources-events-test queue to resources-events topic"

# Subscribe yardi-sync-events queue to resources-events topic
YARDI_QUEUE_ARN="arn:aws:sqs:us-east-1:000000000000:yardi-sync-events"
awslocal sns subscribe \
    --topic-arn "$RESOURCES_TOPIC_ARN" \
    --protocol sqs \
    --notification-endpoint "$YARDI_QUEUE_ARN"
echo "Subscribed yardi-sync-events queue to resources-events topic"

# Subscribe resources-change-logger-events queue to resources-events topic
CHANGE_LOGGER_QUEUE_ARN="arn:aws:sqs:us-east-1:000000000000:resources-change-logger-events"
awslocal sns subscribe \
    --topic-arn "$RESOURCES_TOPIC_ARN" \
    --protocol sqs \
    --notification-endpoint "$CHANGE_LOGGER_QUEUE_ARN"
echo "Subscribed resources-change-logger-events queue to resources-events topic"

echo ""
echo "LocalStack initialization complete!"
echo "Topic ARNs:"
echo "  - resources-events: $RESOURCES_TOPIC_ARN"
echo "  - yardi-sync-failures: arn:aws:sns:us-east-1:000000000000:yardi-sync-failures"
