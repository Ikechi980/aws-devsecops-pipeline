use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SnsEnvelope {
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangeEventMessage {
    pub event_id: Uuid,
    pub resource_type: String,
    pub timestamp: String,
    pub event_type: String,
    pub requester: Requester,
    #[serde(default)]
    pub before: Option<serde_json::Value>,
    #[serde(default)]
    pub after: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Requester {
    EntraUser {
        username: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    IamAssumedRole {
        account_id: String,
        role_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        role_path: Option<String>,
        session_name: String,
    },
    IamUser {
        account_id: String,
        user_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_path: Option<String>,
    },
    IamFederatedUser {
        account_id: String,
        user_name: String,
    },
    IamRoot {
        account_id: String,
    },
    LocalDev,
}

impl Requester {
    pub fn normalize_identity(&self) -> (String, String) {
        match self {
            Requester::EntraUser { username, .. } => ("entra_user".to_string(), username.clone()),
            Requester::IamAssumedRole {
                account_id,
                role_name,
                ..
            } => (
                "iam_assumed_role".to_string(),
                format!("{account_id}:{role_name}"),
            ),
            Requester::IamUser {
                account_id,
                user_name,
                ..
            } => ("iam_user".to_string(), format!("{account_id}:{user_name}")),
            Requester::IamFederatedUser {
                account_id,
                user_name,
            } => (
                "iam_federated_user".to_string(),
                format!("{account_id}:{user_name}"),
            ),
            Requester::IamRoot { account_id } => {
                ("iam_root".to_string(), format!("{account_id}:root"))
            }
            Requester::LocalDev => ("local_dev".to_string(), "local-dev".to_string()),
        }
    }
}
