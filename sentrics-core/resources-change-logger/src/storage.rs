use std::collections::HashMap;

use anyhow::Result;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::types::AttributeValue;

use crate::processor::ChangeLogEntry;

#[derive(Clone)]
pub struct ChangeLogStore {
    client: aws_sdk_dynamodb::Client,
    table_name: String,
}

impl ChangeLogStore {
    pub fn new(client: aws_sdk_dynamodb::Client, table_name: String) -> Self {
        Self { client, table_name }
    }

    pub async fn write_entry(&self, entry: &ChangeLogEntry) -> Result<(), StorageError> {
        let item = entry.to_item().map_err(StorageError::retryable)?;
        let result = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(event_id)")
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(SdkError::ServiceError(service_err))
                if service_err.err().is_conditional_check_failed_exception() =>
            {
                Err(StorageError::Duplicate(entry.event_id.clone()))
            }
            Err(SdkError::ServiceError(service_err))
                if service_err.err().meta().code() == Some("ValidationException") =>
            {
                Err(StorageError::permanent(anyhow::anyhow!(
                    "{}",
                    service_err.err()
                )))
            }
            Err(err) => Err(StorageError::retryable(err)),
        }
    }
}

pub enum StorageError {
    Duplicate(String),
    Retryable(anyhow::Error),
    Permanent(anyhow::Error),
}

impl StorageError {
    fn retryable<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Retryable(err.into())
    }

    fn permanent<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Permanent(err.into())
    }
}

pub fn build_item(entry: &ChangeLogEntry) -> Result<HashMap<String, AttributeValue>> {
    let mut item = HashMap::new();

    let community_pk = format!("COMMUNITY#{}", entry.community_id);
    let timestamp_sk = format!("TS#{}#{}", entry.timestamp, entry.event_id);
    let resource_pk = format!("RESOURCE#{}#{}", entry.resource_type, entry.resource_id);
    let requester_pk = format!("REQUESTER#{}#{}", entry.requester_type, entry.requester_id);

    item.insert("community_pk".to_string(), AttributeValue::S(community_pk));
    item.insert(
        "timestamp_sk".to_string(),
        AttributeValue::S(timestamp_sk.clone()),
    );
    item.insert("resource_pk".to_string(), AttributeValue::S(resource_pk));
    item.insert("requester_pk".to_string(), AttributeValue::S(requester_pk));

    item.insert(
        "event_id".to_string(),
        AttributeValue::S(entry.event_id.clone()),
    );
    item.insert(
        "timestamp".to_string(),
        AttributeValue::S(entry.timestamp.clone()),
    );
    item.insert(
        "resource_type".to_string(),
        AttributeValue::S(entry.resource_type.clone()),
    );
    item.insert(
        "resource_id".to_string(),
        AttributeValue::S(entry.resource_id.clone()),
    );
    item.insert(
        "community_id".to_string(),
        AttributeValue::S(entry.community_id.clone()),
    );
    item.insert(
        "event_type".to_string(),
        AttributeValue::S(entry.event_type.clone()),
    );
    item.insert(
        "requester_type".to_string(),
        AttributeValue::S(entry.requester_type.clone()),
    );
    item.insert(
        "requester_id".to_string(),
        AttributeValue::S(entry.requester_id.clone()),
    );

    item.insert(
        "requester".to_string(),
        json_to_attribute_value(&entry.requester)?,
    );

    let before = entry
        .before
        .as_ref()
        .map(json_to_attribute_value)
        .transpose()?
        .unwrap_or(AttributeValue::Null(true));
    let after = entry
        .after
        .as_ref()
        .map(json_to_attribute_value)
        .transpose()?
        .unwrap_or(AttributeValue::Null(true));

    item.insert("before".to_string(), before);
    item.insert("after".to_string(), after);

    Ok(item)
}

fn json_to_attribute_value(value: &serde_json::Value) -> Result<AttributeValue> {
    match value {
        serde_json::Value::Null => Ok(AttributeValue::Null(true)),
        serde_json::Value::Bool(value) => Ok(AttributeValue::Bool(*value)),
        serde_json::Value::Number(value) => Ok(AttributeValue::N(value.to_string())),
        serde_json::Value::String(value) => Ok(AttributeValue::S(value.clone())),
        serde_json::Value::Array(values) => {
            let mut items = Vec::with_capacity(values.len());
            for value in values {
                items.push(json_to_attribute_value(value)?);
            }
            Ok(AttributeValue::L(items))
        }
        serde_json::Value::Object(values) => {
            let mut map = HashMap::with_capacity(values.len());
            for (key, value) in values {
                map.insert(key.clone(), json_to_attribute_value(value)?);
            }
            Ok(AttributeValue::M(map))
        }
    }
}
