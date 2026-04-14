use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Communities ---
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Community {
    pub id: Uuid,
    pub name: String,
    pub yardi_org_id: Option<String>,
    pub yardi_api_key: Option<String>,
    pub yardi_api_secret: Option<String>,
    pub yardi_api_base_url: Option<String>,
    pub yardi_token_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommunity {
    pub name: Option<String>,
    pub yardi_org_id: Option<String>,
    pub yardi_api_key: Option<String>,
    pub yardi_api_secret: Option<String>,
    pub yardi_api_base_url: Option<String>,
    pub yardi_token_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommunity {
    pub name: Option<String>,
    pub yardi_org_id: Option<String>,
    pub yardi_api_key: Option<String>,
    pub yardi_api_secret: Option<String>,
    pub yardi_api_base_url: Option<String>,
    pub yardi_token_url: Option<String>,
}

// --- Locations ---
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Location {
    pub id: Uuid,
    pub community_id: Uuid,
    pub name: String,
    pub location_type: LocationType,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLocation {
    pub name: Option<String>,
    pub location_type: Option<LocationType>,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLocation {
    pub name: Option<String>,
    pub location_type: Option<LocationType>,
    pub yardi_reference_id: Option<String>,
}

// --- Residents ---
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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

#[derive(Debug, Deserialize)]
pub struct CreateResident {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub location_id: Option<Uuid>,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateResident {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub location_id: Option<Uuid>,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListResidentsParams {
    pub location_id: Option<Uuid>,
}

// --- Location Type ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    Apartment,
}

impl sqlx::Type<sqlx::Postgres> for LocationType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <&str as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for LocationType {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
        match s {
            "apartment" => Ok(LocationType::Apartment),
            other => Err(format!("unknown location_type: {}", other).into()),
        }
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for LocationType {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let s = match self {
            LocationType::Apartment => "apartment",
        };
        <&str as sqlx::Encode<sqlx::Postgres>>::encode(s, buf)
    }
}
