use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct SnsEnvelope {
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChangeEventRouting {
    pub resource_type: String,
    #[serde(flatten)]
    pub event: ChangeEventPayload,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum ChangeEventPayload {
    Create { after: Value },
    Update { after: Value, before: Value },
    Delete { before: Value },
}

impl ChangeEventRouting {
    pub fn event_type(&self) -> &'static str {
        match self.event {
            ChangeEventPayload::Create { .. } => "create",
            ChangeEventPayload::Update { .. } => "update",
            ChangeEventPayload::Delete { .. } => "delete",
        }
    }

    pub fn core_community_id(&self) -> Option<String> {
        let payload = self.event.payload_for_lookup();

        match self.resource_type.as_str() {
            "community" => payload
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            "location" | "resident" => payload
                .get("community_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => payload
                .get("community_id")
                .or_else(|| payload.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }
}

impl ChangeEventPayload {
    pub fn payload_for_lookup(&self) -> &Value {
        match self {
            ChangeEventPayload::Create { after } => after,
            ChangeEventPayload::Update { after, .. } => after,
            ChangeEventPayload::Delete { before } => before,
        }
    }
}
