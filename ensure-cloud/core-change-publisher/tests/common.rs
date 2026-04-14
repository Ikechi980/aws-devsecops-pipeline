#![allow(dead_code)]

use anyhow::{Result, anyhow};
use aws_config::BehaviorVersion;
use serde_json::Value;
use std::env;

fn lambda_invoke_url() -> String {
    env::var("CORE_CHANGE_PUBLISHER_LAMBDA_URL").unwrap_or_else(|_| {
        "http://127.0.0.1:9201/2015-03-31/functions/core-change-publisher/invocations".to_string()
    })
}

pub async fn aws_config_localstack() -> aws_config::SdkConfig {
    let endpoint =
        env::var("AWS_ENDPOINT_URL").unwrap_or_else(|_| "http://127.0.0.1:4666".to_string());

    aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(endpoint)
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_sns::config::Credentials::new(
            "test", "test", None, None, "test",
        ))
        .load()
        .await
}

pub async fn purge_queue_by_name(
    sqs_client: &aws_sdk_sqs::Client,
    queue_name: &str,
) -> Result<String> {
    let queue_url = sqs_client
        .get_queue_url()
        .queue_name(queue_name)
        .send()
        .await?
        .queue_url
        .ok_or_else(|| anyhow!("{queue_name} not found"))?;

    let _ = sqs_client.purge_queue().queue_url(&queue_url).send().await;

    Ok(queue_url)
}

pub async fn drain_queue_by_name(
    sqs_client: &aws_sdk_sqs::Client,
    queue_name: &str,
) -> Result<String> {
    let queue_url = sqs_client
        .get_queue_url()
        .queue_name(queue_name)
        .send()
        .await?
        .queue_url
        .ok_or_else(|| anyhow!("{queue_name} not found"))?;

    for _ in 0..10 {
        let resp = sqs_client
            .receive_message()
            .queue_url(&queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(1)
            .send()
            .await?;

        let messages = resp.messages.unwrap_or_default();
        if messages.is_empty() {
            break;
        }

        for msg in messages {
            if let Some(receipt) = msg.receipt_handle {
                let _ = sqs_client
                    .delete_message()
                    .queue_url(&queue_url)
                    .receipt_handle(receipt)
                    .send()
                    .await;
            }
        }
    }

    Ok(queue_url)
}

pub async fn receive_headend_message(
    sqs_client: &aws_sdk_sqs::Client,
    queue_url: &str,
) -> Result<Option<String>> {
    let resp = sqs_client
        .receive_message()
        .queue_url(queue_url)
        .max_number_of_messages(1)
        .wait_time_seconds(0)
        .send()
        .await?;

    let message = match resp.messages.and_then(|mut msgs| msgs.pop()) {
        Some(msg) => msg,
        None => return Ok(None),
    };

    if let Some(receipt) = message.receipt_handle {
        sqs_client
            .delete_message()
            .queue_url(queue_url)
            .receipt_handle(receipt)
            .send()
            .await?;
    }

    Ok(message.body)
}

#[derive(Debug)]
pub struct HeadendMessage {
    pub target_community_id: String,
    pub message_type: String,
    pub versions: Vec<HeadendVersion>,
    pub data: Value,
}

#[derive(Debug)]
pub struct HeadendVersion {
    pub version: u32,
    pub payload: Value,
}

pub async fn find_headend_message_by_marker(
    sqs_client: &aws_sdk_sqs::Client,
    queue_url: &str,
    marker: &str,
    attempts: usize,
) -> Result<Option<HeadendMessage>> {
    for _ in 0..attempts {
        if let Some(body) = receive_headend_message(sqs_client, queue_url).await?
            && let Ok(envelope) = serde_json::from_str::<Value>(&body)
            && let Some(message) = envelope.get("Message").and_then(Value::as_str)
            && let Ok(published) = serde_json::from_str::<Value>(message)
        {
            let target_community_id =
                match published.get("target_community_id").and_then(Value::as_str) {
                    Some(value) => value.to_string(),
                    None => continue,
                };
            let message_type = match published.get("message_type").and_then(Value::as_str) {
                Some(value) => value.to_string(),
                None => continue,
            };
            let versions = match published.get("versions").and_then(Value::as_array) {
                Some(list) => list,
                None => continue,
            };
            let mut parsed_versions = Vec::new();
            let mut marker_value = String::new();
            let mut data_value: Option<Value> = None;
            for item in versions {
                let version = match item.get("version").and_then(Value::as_u64) {
                    Some(value) => value as u32,
                    None => continue,
                };
                let payload_value = match item.get("payload") {
                    Some(value) => value.clone(),
                    None => continue,
                };
                if payload_value.is_object() {
                    marker_value = payload_value
                        .get("test_marker")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            payload_value
                                .get("after")
                                .and_then(Value::as_object)
                                .and_then(|after| after.get("test_marker"))
                                .and_then(Value::as_str)
                        })
                        .or_else(|| {
                            payload_value
                                .get("before")
                                .and_then(Value::as_object)
                                .and_then(|before| before.get("test_marker"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or_default()
                        .to_string();
                    data_value = Some(payload_value.clone());
                }
                parsed_versions.push(HeadendVersion {
                    version,
                    payload: payload_value,
                });
            }

            if let Some(data) = data_value {
                let marker_value = marker_value;
                if marker_value == marker {
                    return Ok(Some(HeadendMessage {
                        target_community_id,
                        message_type,
                        versions: parsed_versions,
                        data,
                    }));
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    Ok(None)
}

pub async fn invoke_lambda(payload: Value) -> Result<Value> {
    let client = reqwest::Client::new();
    let response = client
        .post(lambda_invoke_url())
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Lambda invoke failed: {status} {body}"));
    }

    let text = response.text().await?;
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }

    Ok(serde_json::from_str(&text)?)
}

pub fn build_change_event(core_community_id: &str) -> String {
    serde_json::json!({
        "event_id": "00000000-0000-0000-0000-000000000000",
        "resource_type": "community",
        "timestamp": "2024-01-15T10:30:00Z",
        "event_type": "update",
        "requester": { "type": "service", "role_name": "local-dev" },
        "after": { "id": core_community_id, "name": "Alpha" },
        "before": { "id": core_community_id, "name": "Alpha" }
    })
    .to_string()
}

pub fn build_change_event_with_marker(core_community_id: &str, marker: &str) -> String {
    serde_json::json!({
        "event_id": "11111111-1111-1111-1111-111111111111",
        "resource_type": "community",
        "timestamp": "2024-01-15T10:30:00Z",
        "event_type": "update",
        "test_marker": marker,
        "requester": { "type": "service", "role_name": "local-dev" },
        "after": { "id": core_community_id, "name": "Alpha", "test_marker": marker },
        "before": { "id": core_community_id, "name": "Alpha", "test_marker": marker }
    })
    .to_string()
}

pub fn build_change_event_missing_id() -> String {
    serde_json::json!({
        "event_id": "00000000-0000-0000-0000-000000000000",
        "resource_type": "community",
        "timestamp": "2024-01-15T10:30:00Z",
        "event_type": "update",
        "requester": { "type": "service", "role_name": "local-dev" },
        "after": { "name": "Alpha" },
        "before": { "name": "Alpha" }
    })
    .to_string()
}

pub fn build_change_event_missing_id_with_marker(marker: &str) -> String {
    serde_json::json!({
        "event_id": "22222222-2222-2222-2222-222222222222",
        "resource_type": "community",
        "timestamp": "2024-01-15T10:30:00Z",
        "event_type": "update",
        "test_marker": marker,
        "requester": { "type": "service", "role_name": "local-dev" },
        "after": { "name": "Alpha", "test_marker": marker },
        "before": { "name": "Alpha", "test_marker": marker }
    })
    .to_string()
}

pub fn wrap_sns_message(message: &str) -> String {
    serde_json::json!({
        "Type": "Notification",
        "MessageId": "test-message",
        "TopicArn": "arn:aws:sns:us-east-1:000000000000:core-change-events",
        "Message": message
    })
    .to_string()
}

pub fn build_sqs_event(records: Vec<(&str, &str)>) -> Value {
    let entries: Vec<Value> = records
        .into_iter()
        .map(|(message_id, body)| {
            serde_json::json!({
                "messageId": message_id,
                "body": body,
            })
        })
        .collect();

    serde_json::json!({ "Records": entries })
}

pub fn failure_ids(response: &Value) -> Vec<String> {
    response
        .get("batchItemFailures")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("itemIdentifier")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
