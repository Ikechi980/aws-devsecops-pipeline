use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Resources API models

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    Apartment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Community {
    pub id: Uuid,
    pub name: String,
    pub yardi_org_id: Option<String>,
    pub yardi_api_key: Option<String>,
    pub yardi_api_secret: Option<String>,
    pub yardi_api_base_url: Option<String>,
    pub yardi_token_url: Option<String>,
}

impl Community {
    pub fn has_yardi_integration(&self) -> bool {
        self.yardi_org_id.is_some()
            && self.yardi_api_key.is_some()
            && self.yardi_api_secret.is_some()
            && self.yardi_api_base_url.is_some()
            && self.yardi_token_url.is_some()
    }

    pub fn yardi_credentials(&self) -> Option<YardiCredentials> {
        match (
            &self.yardi_org_id,
            &self.yardi_api_key,
            &self.yardi_api_secret,
            &self.yardi_api_base_url,
            &self.yardi_token_url,
        ) {
            (
                Some(org_id),
                Some(api_key),
                Some(api_secret),
                Some(api_base_url),
                Some(token_url),
            ) => Some(YardiCredentials {
                organization_id: org_id.clone(),
                api_key: api_key.clone(),
                api_secret: api_secret.clone(),
                api_base_url: api_base_url.clone(),
                token_url: token_url.clone(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct YardiCredentials {
    pub organization_id: String,
    pub api_key: String,
    pub api_secret: String,
    pub api_base_url: String,
    pub token_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub location_type: LocationType,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resident {
    pub id: Uuid,
    pub location_id: Uuid,
    pub community_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub yardi_reference_id: Option<String>,
    pub photo: Option<ResidentPhotoMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResidentPhotoMetadata {
    pub etag: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// Yardi API models

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YardiLocation {
    pub id: String,
    pub name: String,
    pub location_type: YardiLocationType,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum YardiLocationType {
    Site,
    Corridor,
    Room,
    Bed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YardiResident {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub location_ids: Vec<String>,
    pub room_id: Option<String>,
    pub last_updated: Option<String>,
}

impl YardiResident {
    pub fn full_name(&self) -> String {
        if self.first_name.is_empty() {
            self.last_name.clone()
        } else {
            format!("{} {}", self.first_name, self.last_name)
        }
    }
}

// Community state combining resources-api and Yardi data

#[derive(Debug, Clone, Default)]
pub struct CommunityState {
    pub locations: Vec<Location>,
    pub residents: Vec<Resident>,
}

// Change event models (received from resources-api via SQS)

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeEventEnvelope {
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeEvent {
    pub resource_type: String,
    #[serde(flatten)]
    pub event: ChangeEventType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum ChangeEventType {
    Create { after: serde_json::Value },
    Update { after: serde_json::Value },
    Delete { before: serde_json::Value },
}

// Failure notification models

#[derive(Debug, Clone, Serialize)]
pub struct FailureNotification {
    pub failure_type: FailureType,
    pub community_id: Option<Uuid>,
    pub community_name: Option<String>,
    pub message: String,
    pub details: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureType {
    Unreachable,
    CredentialsInvalid,
    DataInvariantViolation,
    UnexpectedResponse,
}
