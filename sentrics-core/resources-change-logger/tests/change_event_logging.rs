use anyhow::{Result, anyhow};
use aws_sdk_dynamodb::types::AttributeValue;
use serde_json::{Value, json};
use uuid::Uuid;

const LAMBDA_INVOKE_URL: &str =
    "http://127.0.0.1:9001/2015-03-31/functions/resources-change-logger/invocations";
fn table_name() -> String {
    std::env::var("CHANGE_LOG_TABLE_NAME").expect("CHANGE_LOG_TABLE_NAME must be set")
}

async fn aws_config_localstack() -> aws_config::SdkConfig {
    aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url("http://127.0.0.1:4566")
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_dynamodb::config::Credentials::new(
            "test", "test", None, None, "test",
        ))
        .load()
        .await
}

async fn dynamodb_client() -> aws_sdk_dynamodb::Client {
    let config = aws_config_localstack().await;
    aws_sdk_dynamodb::Client::new(&config)
}

async fn invoke_lambda(payload: &Value) -> Result<Value> {
    let client = reqwest::Client::new();
    let response = client.post(LAMBDA_INVOKE_URL).json(payload).send().await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("Lambda invocation failed ({status}): {body}"));
    }

    let value: Value = serde_json::from_str(&body)?;
    Ok(value)
}

fn assert_no_batch_failures(response: &Value) {
    let failures = response
        .get("batchItemFailures")
        .and_then(Value::as_array)
        .map(|list| list.len())
        .unwrap_or(0);
    assert_eq!(failures, 0);
}

fn batch_failure_ids(response: &Value) -> Vec<String> {
    response
        .get("batchItemFailures")
        .and_then(Value::as_array)
        .map(|failures| {
            failures
                .iter()
                .filter_map(|item| item.get("itemIdentifier").and_then(Value::as_str))
                .map(|value| value.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn build_sqs_envelope(body: String) -> Value {
    json!({
        "Records": [
            {
                "messageId": "message-1",
                "body": body
            }
        ]
    })
}

fn wrap_sns_message(message: Value) -> String {
    json!({
        "Type": "Notification",
        "Message": message.to_string()
    })
    .to_string()
}

fn build_change_event(
    event_id: Uuid,
    resource_type: &str,
    timestamp: &str,
    requester: Value,
    before: Option<Value>,
    after: Option<Value>,
) -> Value {
    let mut payload = json!({
        "event_id": event_id,
        "resource_type": resource_type,
        "timestamp": timestamp,
        "event_type": "update",
        "requester": requester
    });

    if let Some(before) = before {
        payload["before"] = before;
    }
    if let Some(after) = after {
        payload["after"] = after;
    }

    payload
}

fn attr_string(
    item: &std::collections::HashMap<String, AttributeValue>,
    key: &str,
) -> Option<String> {
    item.get(key).and_then(|value| match value {
        AttributeValue::S(value) => Some(value.clone()),
        _ => None,
    })
}

fn attr_map(
    item: &std::collections::HashMap<String, AttributeValue>,
    key: &str,
) -> Option<std::collections::HashMap<String, AttributeValue>> {
    item.get(key).and_then(|value| match value {
        AttributeValue::M(map) => Some(map.clone()),
        _ => None,
    })
}

fn attr_null(item: &std::collections::HashMap<String, AttributeValue>, key: &str) -> Option<bool> {
    item.get(key).and_then(|value| match value {
        AttributeValue::Null(value) => Some(*value),
        _ => None,
    })
}

async fn fetch_item(
    client: &aws_sdk_dynamodb::Client,
    community_id: &str,
    timestamp: &str,
    event_id: Uuid,
) -> Result<Option<std::collections::HashMap<String, AttributeValue>>> {
    let table_name = table_name();
    let community_pk = format!("COMMUNITY#{}", community_id);
    let timestamp_sk = format!("TS#{}#{}", timestamp, event_id);

    let response = client
        .get_item()
        .table_name(table_name)
        .key("community_pk", AttributeValue::S(community_pk))
        .key("timestamp_sk", AttributeValue::S(timestamp_sk))
        .send()
        .await?;

    Ok(response.item)
}

async fn query_by_resource(
    client: &aws_sdk_dynamodb::Client,
    resource_type: &str,
    resource_id: &str,
    timestamp: &str,
    event_id: Uuid,
) -> Result<usize> {
    let table_name = table_name();
    let resource_pk = format!("RESOURCE#{}#{}", resource_type, resource_id);
    let timestamp_sk = format!("TS#{}#{}", timestamp, event_id);

    let response = client
        .query()
        .table_name(table_name)
        .index_name("by_resource")
        .key_condition_expression("resource_pk = :resource_pk AND timestamp_sk = :timestamp_sk")
        .expression_attribute_values(":resource_pk", AttributeValue::S(resource_pk))
        .expression_attribute_values(":timestamp_sk", AttributeValue::S(timestamp_sk))
        .send()
        .await?;

    Ok(response.count as usize)
}

async fn query_by_requester(
    client: &aws_sdk_dynamodb::Client,
    requester_type: &str,
    requester_id: &str,
    timestamp: &str,
    event_id: Uuid,
) -> Result<usize> {
    let table_name = table_name();
    let requester_pk = format!("REQUESTER#{}#{}", requester_type, requester_id);
    let timestamp_sk = format!("TS#{}#{}", timestamp, event_id);

    let response = client
        .query()
        .table_name(table_name)
        .index_name("by_requester")
        .key_condition_expression("requester_pk = :requester_pk AND timestamp_sk = :timestamp_sk")
        .expression_attribute_values(":requester_pk", AttributeValue::S(requester_pk))
        .expression_attribute_values(":timestamp_sk", AttributeValue::S(timestamp_sk))
        .send()
        .await?;

    Ok(response.count as usize)
}

#[tokio::test]
async fn change_event_logging() -> Result<()> {
    let client = dynamodb_client().await;

    println!("Running resources-change-logger integration tests...");

    let timestamp = "2024-01-15T10:30:00Z";
    let community_id = Uuid::new_v4().to_string();
    let resource_id = Uuid::new_v4().to_string();
    let event_id = Uuid::new_v4();

    println!("  test_write_and_read_back...");
    let requester = json!({ "type": "local_dev" });
    let after = json!({
        "id": resource_id,
        "community_id": community_id,
        "name": "Alpha"
    });

    let payload = build_change_event(
        event_id,
        "location",
        timestamp,
        requester,
        None,
        Some(after),
    );

    let sns_body = wrap_sns_message(payload);
    let sqs_payload = build_sqs_envelope(sns_body);

    let response = invoke_lambda(&sqs_payload).await?;
    assert_no_batch_failures(&response);

    let item = fetch_item(&client, &community_id, timestamp, event_id).await?;
    let item = item.expect("Expected item to be written");
    assert_eq!(
        attr_string(&item, "resource_id").as_deref(),
        Some(resource_id.as_str())
    );
    assert_eq!(
        attr_string(&item, "community_id").as_deref(),
        Some(community_id.as_str())
    );
    assert_eq!(
        attr_string(&item, "requester_type").as_deref(),
        Some("local_dev")
    );
    assert_eq!(
        attr_string(&item, "requester_id").as_deref(),
        Some("local-dev")
    );
    assert_eq!(
        attr_string(&item, "resource_type").as_deref(),
        Some("location")
    );
    assert_eq!(attr_string(&item, "event_type").as_deref(), Some("update"));
    assert_eq!(attr_string(&item, "timestamp").as_deref(), Some(timestamp));

    let requester = attr_map(&item, "requester").expect("Expected requester map");
    assert_eq!(
        attr_string(&requester, "type").as_deref(),
        Some("local_dev")
    );

    let after_item = attr_map(&item, "after").expect("Expected after map");
    assert_eq!(
        attr_string(&after_item, "community_id").as_deref(),
        Some(community_id.as_str())
    );
    assert_eq!(attr_string(&after_item, "name").as_deref(), Some("Alpha"));
    assert!(attr_map(&item, "before").is_none());
    assert_eq!(attr_null(&item, "before"), Some(true));

    let count = query_by_resource(&client, "location", &resource_id, timestamp, event_id).await?;
    assert_eq!(count, 1);

    println!("  test_resident_photo_metadata_event...");
    let photo_event_id = Uuid::new_v4();
    let photo_resident_id = Uuid::new_v4().to_string();
    let photo_payload = build_change_event(
        photo_event_id,
        "resident",
        timestamp,
        json!({ "type": "local_dev" }),
        Some(json!({
            "id": photo_resident_id,
            "community_id": community_id,
            "first_name": "Photo",
            "last_name": "Resident",
            "photo": null
        })),
        Some(json!({
            "id": photo_resident_id,
            "community_id": community_id,
            "first_name": "Photo",
            "last_name": "Resident",
            "photo": {
                "etag": "sha256:abc123",
                "content_type": "image/png",
                "size_bytes": 1234,
                "updated_at": "2026-01-01T00:00:00Z"
            }
        })),
    );

    let photo_sqs = build_sqs_envelope(wrap_sns_message(photo_payload));
    let photo_response = invoke_lambda(&photo_sqs).await?;
    assert_no_batch_failures(&photo_response);

    let photo_item = fetch_item(&client, &community_id, timestamp, photo_event_id).await?;
    let photo_item = photo_item.expect("Expected resident photo item to be written");
    assert_eq!(
        attr_string(&photo_item, "resource_type").as_deref(),
        Some("resident")
    );
    let photo_after = attr_map(&photo_item, "after").expect("Expected after map");
    let photo_meta = attr_map(&photo_after, "photo").expect("Expected photo metadata map");
    assert_eq!(
        attr_string(&photo_meta, "etag").as_deref(),
        Some("sha256:abc123")
    );
    assert_eq!(
        attr_string(&photo_meta, "content_type").as_deref(),
        Some("image/png")
    );
    assert_eq!(
        attr_string(&photo_meta, "updated_at").as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
    assert!(attr_string(&photo_meta, "image_data").is_none());

    println!("  test_non_sns_body_dropped...");
    // Raw change-event JSON should be treated as a permanent failure and dropped.
    let raw_event_id = Uuid::new_v4();
    let raw_resource_id = Uuid::new_v4().to_string();
    let raw_payload = build_change_event(
        raw_event_id,
        "resident",
        timestamp,
        json!({ "type": "local_dev" }),
        None,
        Some(json!({
            "id": raw_resource_id,
            "community_id": community_id,
            "first_name": "Gamma",
            "last_name": "Resident"
        })),
    );
    let raw_sqs = build_sqs_envelope(raw_payload.to_string());
    let raw_response = invoke_lambda(&raw_sqs).await?;
    assert!(batch_failure_ids(&raw_response).is_empty());
    let raw_item = fetch_item(&client, &community_id, timestamp, raw_event_id).await?;
    assert!(raw_item.is_none());

    println!("  test_idempotent_duplicate_event...");
    let duplicate_response = invoke_lambda(&sqs_payload).await?;
    assert_no_batch_failures(&duplicate_response);

    let count = query_by_resource(&client, "location", &resource_id, timestamp, event_id).await?;
    assert_eq!(count, 1);

    println!("  test_missing_community_id_dropped...");
    let invalid_event_id = Uuid::new_v4();
    let invalid_resource_id = "missing-community".to_string();
    let invalid_payload = build_change_event(
        invalid_event_id,
        "location",
        timestamp,
        json!({ "type": "local_dev" }),
        None,
        Some(json!({ "id": invalid_resource_id })),
    );

    let invalid_sqs = build_sqs_envelope(wrap_sns_message(invalid_payload));
    let invalid_response = invoke_lambda(&invalid_sqs).await?;
    assert_no_batch_failures(&invalid_response);

    let invalid_count = query_by_resource(
        &client,
        "location",
        &invalid_resource_id,
        timestamp,
        invalid_event_id,
    )
    .await?;
    assert_eq!(invalid_count, 0);

    println!("  test_invalid_timestamp_dropped...");
    let bad_time_event_id = Uuid::new_v4();
    let bad_time_payload = build_change_event(
        bad_time_event_id,
        "location",
        "not-a-timestamp",
        json!({ "type": "local_dev" }),
        None,
        Some(json!({
            "id": Uuid::new_v4().to_string(),
            "community_id": community_id
        })),
    );
    let bad_time_sqs = build_sqs_envelope(wrap_sns_message(bad_time_payload));
    let bad_time_response = invoke_lambda(&bad_time_sqs).await?;
    assert_no_batch_failures(&bad_time_response);
    let bad_time_item =
        fetch_item(&client, &community_id, "not-a-timestamp", bad_time_event_id).await?;
    assert!(bad_time_item.is_none());

    println!("  test_delete_event_before_only...");
    let delete_event_id = Uuid::new_v4();
    let delete_payload = build_change_event(
        delete_event_id,
        "location",
        timestamp,
        json!({ "type": "local_dev" }),
        Some(json!({
            "id": Uuid::new_v4().to_string(),
            "community_id": community_id,
            "name": "ToRemove"
        })),
        None,
    );
    let delete_sqs = build_sqs_envelope(wrap_sns_message(delete_payload));
    let delete_response = invoke_lambda(&delete_sqs).await?;
    assert_no_batch_failures(&delete_response);
    let delete_item = fetch_item(&client, &community_id, timestamp, delete_event_id).await?;
    let delete_item = delete_item.expect("Expected delete item");
    let before_item = attr_map(&delete_item, "before").expect("Expected before map");
    assert_eq!(
        attr_string(&before_item, "name").as_deref(),
        Some("ToRemove")
    );
    assert_eq!(attr_null(&delete_item, "after"), Some(true));

    println!("  test_community_event_and_requester_normalization...");
    let community_event_id = Uuid::new_v4();
    let community_resource_id = Uuid::new_v4().to_string();
    let community_payload = build_change_event(
        community_event_id,
        "community",
        timestamp,
        json!({
            "type": "iam_assumed_role",
            "account_id": "123456789012",
            "role_name": "CoreRole",
            "session_name": "session"
        }),
        Some(json!({ "id": community_resource_id, "name": "Alpha" })),
        Some(json!({ "id": community_resource_id, "name": "Bravo" })),
    );

    let community_sqs = build_sqs_envelope(wrap_sns_message(community_payload));
    let community_response = invoke_lambda(&community_sqs).await?;
    assert_no_batch_failures(&community_response);

    let community_item = fetch_item(
        &client,
        &community_resource_id,
        timestamp,
        community_event_id,
    )
    .await?;
    let community_item = community_item.expect("Expected community item");
    assert_eq!(
        attr_string(&community_item, "community_id").as_deref(),
        Some(community_resource_id.as_str())
    );
    assert_eq!(
        attr_string(&community_item, "requester_type").as_deref(),
        Some("iam_assumed_role")
    );
    assert_eq!(
        attr_string(&community_item, "requester_id").as_deref(),
        Some("123456789012:CoreRole")
    );
    let before_item = attr_map(&community_item, "before").expect("Expected before map");
    assert_eq!(attr_string(&before_item, "name").as_deref(), Some("Alpha"));
    let after_item = attr_map(&community_item, "after").expect("Expected after map");
    assert_eq!(attr_string(&after_item, "name").as_deref(), Some("Bravo"));

    let requester_count = query_by_requester(
        &client,
        "iam_assumed_role",
        "123456789012:CoreRole",
        timestamp,
        community_event_id,
    )
    .await?;
    assert_eq!(requester_count, 1);

    println!("  test_permanent_failure_on_oversize_item...");
    // Oversized payload should trigger DynamoDB validation and be dropped as a permanent failure.
    let oversize_event_id = Uuid::new_v4();
    let large_value = "x".repeat(450_000);
    let oversize_payload = build_change_event(
        oversize_event_id,
        "resident",
        timestamp,
        json!({ "type": "local_dev" }),
        None,
        Some(json!({
            "id": Uuid::new_v4().to_string(),
            "community_id": community_id,
            "notes": large_value
        })),
    );
    let oversize_sqs = json!({
        "Records": [
            {
                "messageId": "oversize-message",
                "body": wrap_sns_message(oversize_payload)
            }
        ]
    });
    let oversize_response = invoke_lambda(&oversize_sqs).await?;
    let failure_ids = batch_failure_ids(&oversize_response);
    assert!(failure_ids.is_empty());
    let oversize_item = fetch_item(&client, &community_id, timestamp, oversize_event_id).await?;
    assert!(oversize_item.is_none());

    println!("  test_malformed_json_dropped...");
    // Malformed JSON should be treated as a permanent failure and not be retried.
    let malformed_sqs = json!({
        "Records": [
            {
                "messageId": "malformed-message",
                "body": "{not-json}"
            }
        ]
    });
    let malformed_response = invoke_lambda(&malformed_sqs).await?;
    assert!(batch_failure_ids(&malformed_response).is_empty());

    println!("All integration tests completed!");
    Ok(())
}
