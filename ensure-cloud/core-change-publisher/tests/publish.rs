use anyhow::{Result, anyhow};
use serde_json::Value;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

mod common;
use common::{
    aws_config_localstack, build_change_event_missing_id_with_marker,
    build_change_event_with_marker, build_sqs_event, drain_queue_by_name, failure_ids,
    find_headend_message_by_marker, invoke_lambda, wrap_sns_message,
};

fn unique_marker(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}-{nanos}")
}

#[tokio::test]
async fn end_to_end_publish_scenarios() -> Result<()> {
    let _ = dotenvy::dotenv();

    let aws_config = aws_config_localstack().await;
    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);

    let headend_queue_url = drain_queue_by_name(&sqs_client, "headend-test-queue").await?;
    let _ = drain_queue_by_name(&sqs_client, "core-change-events-queue").await?;

    // Happy path
    println!("Scenario: happy path publish");
    let marker = unique_marker("happy");
    let event_body =
        build_change_event_with_marker("11111111-1111-1111-1111-111111111111", &marker);
    let sns_body = wrap_sns_message(&event_body);
    let response = invoke_lambda(build_sqs_event(vec![("happy", &sns_body)])).await?;
    assert!(failure_ids(&response).is_empty());

    let found = find_headend_message_by_marker(&sqs_client, &headend_queue_url, &marker, 10)
        .await?
        .ok_or_else(|| anyhow!("No headend message received"))?;

    assert_eq!(found.target_community_id, "alpha");
    assert_eq!(found.message_type, "core_change_event");
    assert_eq!(found.versions.len(), 1);
    assert_eq!(found.versions[0].version, 1);
    assert_eq!(
        found
            .data
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "update"
    );
    assert_eq!(
        found
            .data
            .get("event_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "11111111-1111-1111-1111-111111111111"
    );

    // Resident photo update keeps before/after payload fields intact.
    println!("Scenario: resident photo update preserves before/after");
    let marker = unique_marker("resident-photo");
    let event_body = json!({
        "event_id": "33333333-3333-3333-3333-333333333333",
        "resource_type": "resident",
        "timestamp": "2026-01-15T10:30:00Z",
        "event_type": "update",
        "requester": { "type": "service", "role_name": "local-dev" },
        "after": {
            "id": "dddddddd-dddd-dddd-dddd-dddddddddddd",
            "community_id": "11111111-1111-1111-1111-111111111111",
            "first_name": "Alice",
            "last_name": "Alpha",
            "test_marker": marker,
            "photo": null
        },
        "before": {
            "id": "dddddddd-dddd-dddd-dddd-dddddddddddd",
            "community_id": "11111111-1111-1111-1111-111111111111",
            "first_name": "Alice",
            "last_name": "Alpha",
            "test_marker": marker,
            "photo": {
                "etag": "sha256:abc123",
                "content_type": "image/png",
                "size_bytes": 123,
                "updated_at": "2026-01-15T09:00:00Z"
            }
        }
    })
    .to_string();

    let sns_body = wrap_sns_message(&event_body);
    let response = invoke_lambda(build_sqs_event(vec![("resident-photo", &sns_body)])).await?;
    assert!(failure_ids(&response).is_empty());

    let found = find_headend_message_by_marker(&sqs_client, &headend_queue_url, &marker, 10)
        .await?
        .ok_or_else(|| anyhow!("No headend message received for resident photo event"))?;

    assert_eq!(found.target_community_id, "alpha");
    assert_eq!(found.message_type, "core_change_event");
    assert_eq!(
        found
            .data
            .get("resource_type")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "resident"
    );
    assert!(
        found
            .data
            .get("before")
            .and_then(|v| v.get("first_name"))
            .and_then(Value::as_str)
            .is_some_and(|v| v == "Alice")
    );
    assert!(
        found
            .data
            .get("after")
            .and_then(|v| v.get("last_name"))
            .and_then(Value::as_str)
            .is_some_and(|v| v == "Alpha")
    );
    assert!(
        found
            .data
            .get("before")
            .and_then(|v| v.get("name"))
            .is_none()
    );
    assert!(
        found
            .data
            .get("before")
            .and_then(|v| v.get("photo"))
            .and_then(Value::as_object)
            .is_some()
    );
    assert!(
        found
            .data
            .get("after")
            .and_then(|v| v.get("photo"))
            .is_some_and(Value::is_null)
    );

    // Unmapped core community
    println!("Scenario: unmapped core community");
    let marker = unique_marker("unmapped");
    let event_body =
        build_change_event_with_marker("99999999-9999-9999-9999-999999999999", &marker);
    let sns_body = wrap_sns_message(&event_body);
    let response = invoke_lambda(build_sqs_event(vec![("unmapped", &sns_body)])).await?;
    assert!(failure_ids(&response).is_empty());

    let found = find_headend_message_by_marker(&sqs_client, &headend_queue_url, &marker, 6).await?;
    assert!(found.is_none());

    // Missing community id
    println!("Scenario: missing community id");
    let marker = unique_marker("missing");
    let event_body = build_change_event_missing_id_with_marker(&marker);
    let sns_body = wrap_sns_message(&event_body);
    let response = invoke_lambda(build_sqs_event(vec![("missing", &sns_body)])).await?;
    assert!(failure_ids(&response).is_empty());

    let found = find_headend_message_by_marker(&sqs_client, &headend_queue_url, &marker, 6).await?;
    assert!(found.is_none());

    // Non-SNS body is dropped (permanent, no retry)
    println!("Scenario: non-SNS body dropped");
    let marker = unique_marker("non-sns");
    let event_body =
        build_change_event_with_marker("11111111-1111-1111-1111-111111111111", &marker);
    let response = invoke_lambda(build_sqs_event(vec![("non-sns", &event_body)])).await?;
    assert!(failure_ids(&response).is_empty());

    let found = find_headend_message_by_marker(&sqs_client, &headend_queue_url, &marker, 6).await?;
    assert!(found.is_none());

    // Invalid JSON is permanent
    println!("Scenario: invalid JSON permanent failure");
    let sns_body = wrap_sns_message("not-json");
    let response = invoke_lambda(build_sqs_event(vec![("invalid", &sns_body)])).await?;
    assert!(failure_ids(&response).is_empty());

    let found =
        find_headend_message_by_marker(&sqs_client, &headend_queue_url, "invalid", 4).await?;
    assert!(found.is_none());

    // Mixed batch
    println!("Scenario: mixed batch valid + invalid");
    let marker = unique_marker("mixed");
    let event_body =
        build_change_event_with_marker("11111111-1111-1111-1111-111111111111", &marker);
    let sns_body = wrap_sns_message(&event_body);

    let response = invoke_lambda(build_sqs_event(vec![
        ("mixed-valid", &sns_body),
        ("mixed-invalid", "not-json"),
    ]))
    .await?;
    assert!(failure_ids(&response).is_empty());

    let found = find_headend_message_by_marker(&sqs_client, &headend_queue_url, &marker, 10)
        .await?
        .ok_or_else(|| anyhow!("No headend message received"))?;
    assert_eq!(found.target_community_id, "alpha");
    assert_eq!(found.message_type, "core_change_event");
    assert_eq!(found.versions.len(), 1);
    assert_eq!(found.versions[0].version, 1);
    assert_eq!(
        found
            .data
            .get("event_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "11111111-1111-1111-1111-111111111111"
    );

    Ok(())
}
