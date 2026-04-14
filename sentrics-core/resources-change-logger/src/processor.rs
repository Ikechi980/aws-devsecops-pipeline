use std::collections::HashMap;

use anyhow::{Result, anyhow};
use aws_lambda_events::event::sqs::{SqsBatchResponse, SqsEvent, SqsMessage};
use chrono::{DateTime, SecondsFormat};

use crate::events::{ChangeEventMessage, SnsEnvelope};
use crate::storage::{ChangeLogStore, StorageError, build_item};

#[derive(Clone)]
pub struct ChangeLogger {
    store: ChangeLogStore,
}

impl ChangeLogger {
    pub fn new(client: aws_sdk_dynamodb::Client, table_name: String) -> Self {
        Self {
            store: ChangeLogStore::new(client, table_name),
        }
    }

    pub async fn handle_sqs_event(&self, event: SqsEvent) -> SqsBatchResponse {
        let mut response = SqsBatchResponse::default();

        for record in event.records {
            match self.process_message(&record).await {
                Ok(()) => {}
                Err(ProcessError::Retryable(err)) => {
                    let message_id = record
                        .message_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    tracing::error!(%message_id, error = ?err, "Failed to process SQS message");
                    response.add_failure(message_id);
                }
                Err(ProcessError::Permanent(err)) => {
                    let message_id = record
                        .message_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    tracing::warn!(%message_id, error = ?err, "Dropping non-retryable SQS message");
                }
            }
        }

        response
    }

    async fn process_message(&self, record: &SqsMessage) -> Result<(), ProcessError> {
        let body = record
            .body
            .as_deref()
            .ok_or_else(|| ProcessError::permanent(anyhow!("SQS message missing body")))?;

        self.process_body(record.message_id.as_deref(), body).await
    }

    async fn process_body(&self, message_id: Option<&str>, body: &str) -> Result<(), ProcessError> {
        let raw_message = extract_sns_message(body).map_err(ProcessError::permanent)?;
        let event: ChangeEventMessage =
            serde_json::from_str(&raw_message).map_err(ProcessError::permanent)?;

        let entry = ChangeLogEntry::from_event(event).map_err(ProcessError::permanent)?;

        match self.store.write_entry(&entry).await {
            Ok(()) => {}
            Err(StorageError::Duplicate(event_id)) => {
                tracing::info!(
                    event_id = %event_id,
                    message_id = message_id.unwrap_or("unknown"),
                    "Duplicate event ignored"
                );
                return Ok(());
            }
            Err(StorageError::Permanent(err)) => return Err(ProcessError::permanent(err)),
            Err(StorageError::Retryable(err)) => return Err(ProcessError::retryable(err)),
        }

        tracing::info!(
            event_id = %entry.event_id,
            resource_type = %entry.resource_type,
            resource_id = %entry.resource_id,
            timestamp = %entry.timestamp,
            message_id = message_id.unwrap_or("unknown"),
            "Stored change event"
        );

        Ok(())
    }
}

pub struct ChangeLogEntry {
    pub event_id: String,
    pub resource_type: String,
    pub timestamp: String,
    pub event_type: String,
    pub resource_id: String,
    pub community_id: String,
    pub requester_type: String,
    pub requester_id: String,
    pub requester: serde_json::Value,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}

impl ChangeLogEntry {
    pub fn from_event(event: ChangeEventMessage) -> Result<Self> {
        let timestamp = normalize_timestamp(&event.timestamp)?;
        let resource_id = extract_resource_id(&event)?;
        let community_id = extract_community_id(&event, &resource_id)?;
        let (requester_type, requester_id) = event.requester.normalize_identity();
        let requester = serde_json::to_value(&event.requester)?;

        Ok(Self {
            event_id: event.event_id.to_string(),
            resource_type: event.resource_type,
            timestamp,
            event_type: event.event_type,
            resource_id,
            community_id,
            requester_type,
            requester_id,
            requester,
            before: event.before,
            after: event.after,
        })
    }

    pub fn to_item(&self) -> Result<HashMap<String, aws_sdk_dynamodb::types::AttributeValue>> {
        build_item(self)
    }
}

fn extract_sns_message(body: &str) -> Result<String> {
    let envelope = serde_json::from_str::<SnsEnvelope>(body)
        .map_err(|err| anyhow!("Expected SNS envelope: {err}"))?;
    Ok(envelope.message)
}

fn normalize_timestamp(timestamp: &str) -> Result<String> {
    let parsed = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|err| anyhow!("Invalid RFC3339 timestamp: {err}"))?;
    Ok(parsed.to_rfc3339_opts(SecondsFormat::AutoSi, true))
}

fn extract_resource_id(event: &ChangeEventMessage) -> Result<String> {
    event
        .before
        .as_ref()
        .and_then(|value| value.get("id").and_then(|id| id.as_str()))
        .or_else(|| {
            event
                .after
                .as_ref()
                .and_then(|value| value.get("id").and_then(|id| id.as_str()))
        })
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("Missing resource id in change event"))
}

fn extract_community_id(event: &ChangeEventMessage, resource_id: &str) -> Result<String> {
    if event.resource_type == "community" {
        return Ok(resource_id.to_string());
    }

    event
        .before
        .as_ref()
        .and_then(|value| value.get("community_id").and_then(|id| id.as_str()))
        .or_else(|| {
            event
                .after
                .as_ref()
                .and_then(|value| value.get("community_id").and_then(|id| id.as_str()))
        })
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("Missing community id in change event"))
}

#[derive(Debug)]
pub enum ProcessError {
    Retryable(anyhow::Error),
    Permanent(anyhow::Error),
}

impl ProcessError {
    fn retryable<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Retryable(err.into())
    }

    fn permanent<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Permanent(err.into())
    }
}
