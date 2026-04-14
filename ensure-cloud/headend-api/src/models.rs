use chrono::{DateTime, SecondsFormat, Utc};
use mongodb::bson;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Core Resources Models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: Uuid,
    pub name: String,
    pub yardi_org_id: Option<String>,
    pub yardi_api_key: Option<String>,
    pub yardi_api_secret: Option<String>,
    pub yardi_api_base_url: Option<String>,
    pub yardi_token_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub location_type: LocationType,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

// --- Location Type ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    Apartment,
}

// --- Events Models ---

fn serialize_datetime<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn deserialize_uuid<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bson_value = bson::Bson::deserialize(deserializer)?;

    match bson_value {
        bson::Bson::String(s) => Uuid::parse_str(&s).map_err(serde::de::Error::custom),
        bson::Bson::Binary(binary) if binary.subtype == bson::spec::BinarySubtype::Uuid => {
            Uuid::from_slice(&binary.bytes).map_err(serde::de::Error::custom)
        }
        _ => Err(serde::de::Error::custom(format!(
            "Cannot deserialize UUID from BSON type: {:?}",
            bson_value
        ))),
    }
}

fn deserialize_bson_datetime<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bson_dt = bson::DateTime::deserialize(deserializer)?;
    bson_dt
        .try_to_rfc3339_string()
        .map_err(serde::de::Error::custom)?
        .parse::<DateTime<Utc>>()
        .map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalEvent {
    #[serde(rename = "_id", deserialize_with = "deserialize_uuid")]
    pub id: Uuid,
    pub community_id: String,
    pub payload_type: String,
    pub payload: serde_json::Value,
    #[serde(
        serialize_with = "serialize_datetime",
        deserialize_with = "deserialize_bson_datetime"
    )]
    pub created_at: DateTime<Utc>,
    #[serde(
        serialize_with = "serialize_datetime",
        deserialize_with = "deserialize_bson_datetime"
    )]
    pub inserted_at: DateTime<Utc>,
}
