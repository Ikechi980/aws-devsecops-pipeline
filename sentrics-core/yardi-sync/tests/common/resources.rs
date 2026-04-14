//! Helper functions for interacting with resources-api in tests.

use std::env;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client for interacting with resources-api in tests.
pub struct ResourcesApiClient {
    http: reqwest::Client,
    base_url: String,
}

impl ResourcesApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            base_url: base_url.to_string(),
        }
    }

    /// Health check for resources-api.
    pub async fn health_check(&self) -> anyhow::Result<bool> {
        match self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
        {
            Ok(r) => Ok(r.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Creates a community.
    pub async fn create_community(&self, community: &CreateCommunity) -> anyhow::Result<Community> {
        let response = self
            .http
            .post(format!("{}/communities", self.base_url))
            .json(community)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to create community: {} - {}", status, body);
        }

        Ok(response.json().await?)
    }

    /// Gets a community by ID.
    pub async fn get_community(&self, id: Uuid) -> anyhow::Result<Option<Community>> {
        let response = self
            .http
            .get(format!("{}/communities/{}", self.base_url, id))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get community: {} - {}", status, body);
        }

        Ok(Some(response.json().await?))
    }

    /// Updates a community.
    pub async fn update_community(
        &self,
        id: Uuid,
        update: &UpdateCommunity,
    ) -> anyhow::Result<Community> {
        let response = self
            .http
            .put(format!("{}/communities/{}", self.base_url, id))
            .json(update)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to update community: {} - {}", status, body);
        }

        Ok(response.json().await?)
    }

    /// Deletes a community.
    pub async fn delete_community(&self, id: Uuid) -> anyhow::Result<()> {
        let response = self
            .http
            .delete(format!("{}/communities/{}", self.base_url, id))
            .send()
            .await?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to delete community: {} - {}", status, body);
        }

        Ok(())
    }

    /// Lists locations for a community.
    pub async fn list_locations(&self, community_id: Uuid) -> anyhow::Result<Vec<Location>> {
        let response = self
            .http
            .get(format!(
                "{}/communities/{}/locations",
                self.base_url, community_id
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list locations: {} - {}", status, body);
        }

        Ok(response.json().await?)
    }

    /// Lists locations for a community, returning None if the community is missing.
    pub async fn list_locations_optional(
        &self,
        community_id: Uuid,
    ) -> anyhow::Result<Option<Vec<Location>>> {
        let response = self
            .http
            .get(format!(
                "{}/communities/{}/locations",
                self.base_url, community_id
            ))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list locations: {} - {}", status, body);
        }

        Ok(Some(response.json().await?))
    }

    /// Creates a location.
    pub async fn create_location(
        &self,
        community_id: Uuid,
        name: &str,
        yardi_reference_id: &str,
    ) -> anyhow::Result<Location> {
        let response = self
            .http
            .post(format!(
                "{}/communities/{}/locations",
                self.base_url, community_id
            ))
            .json(&serde_json::json!({
                "name": name,
                "location_type": "apartment",
                "yardi_reference_id": yardi_reference_id
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to create location: {} - {}", status, body);
        }

        Ok(response.json().await?)
    }

    /// Deletes a location.
    #[allow(dead_code)]
    pub async fn delete_location(&self, community_id: Uuid, id: Uuid) -> anyhow::Result<()> {
        let response = self
            .http
            .delete(format!(
                "{}/communities/{}/locations/{}",
                self.base_url, community_id, id
            ))
            .send()
            .await?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to delete location: {} - {}", status, body);
        }

        Ok(())
    }

    /// Lists residents for a community.
    pub async fn list_residents(&self, community_id: Uuid) -> anyhow::Result<Vec<Resident>> {
        let response = self
            .http
            .get(format!(
                "{}/communities/{}/residents",
                self.base_url, community_id
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list residents: {} - {}", status, body);
        }

        Ok(response.json().await?)
    }

    /// Lists residents for a community, returning None if the community is missing.
    pub async fn list_residents_optional(
        &self,
        community_id: Uuid,
    ) -> anyhow::Result<Option<Vec<Resident>>> {
        let response = self
            .http
            .get(format!(
                "{}/communities/{}/residents",
                self.base_url, community_id
            ))
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to list residents: {} - {}", status, body);
        }

        Ok(Some(response.json().await?))
    }

    /// Creates a resident.
    pub async fn create_resident(
        &self,
        community_id: Uuid,
        location_id: Uuid,
        first_name: &str,
        last_name: &str,
        yardi_reference_id: &str,
    ) -> anyhow::Result<Resident> {
        let response = self
            .http
            .post(format!(
                "{}/communities/{}/residents",
                self.base_url, community_id
            ))
            .json(&serde_json::json!({
                "first_name": first_name,
                "last_name": last_name,
                "location_id": location_id,
                "yardi_reference_id": yardi_reference_id
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to create resident: {} - {}", status, body);
        }

        Ok(response.json().await?)
    }

    /// Deletes a resident.
    #[allow(dead_code)]
    pub async fn delete_resident(&self, community_id: Uuid, id: Uuid) -> anyhow::Result<()> {
        let response = self
            .http
            .delete(format!(
                "{}/communities/{}/residents/{}",
                self.base_url, community_id, id
            ))
            .send()
            .await?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to delete resident: {} - {}", status, body);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateCommunity {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_api_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_api_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_token_url: Option<String>,
}

impl CreateCommunity {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            yardi_org_id: None,
            yardi_api_key: None,
            yardi_api_secret: None,
            yardi_api_base_url: None,
            yardi_token_url: None,
        }
    }

    pub fn with_yardi(name: &str, org_id: &str, api_key: &str, api_secret: &str) -> Self {
        let (api_base_url, token_url) = mock_yardi_urls();
        Self {
            name: name.to_string(),
            yardi_org_id: Some(org_id.to_string()),
            yardi_api_key: Some(api_key.to_string()),
            yardi_api_secret: Some(api_secret.to_string()),
            yardi_api_base_url: Some(api_base_url),
            yardi_token_url: Some(token_url),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCommunity {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_api_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_api_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yardi_token_url: Option<String>,
}

impl UpdateCommunity {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            yardi_org_id: None,
            yardi_api_key: None,
            yardi_api_secret: None,
            yardi_api_base_url: None,
            yardi_token_url: None,
        }
    }

    pub fn with_yardi(name: &str, org_id: &str, api_key: &str, api_secret: &str) -> Self {
        let (api_base_url, token_url) = mock_yardi_urls();
        Self {
            name: name.to_string(),
            yardi_org_id: Some(org_id.to_string()),
            yardi_api_key: Some(api_key.to_string()),
            yardi_api_secret: Some(api_secret.to_string()),
            yardi_api_base_url: Some(api_base_url),
            yardi_token_url: Some(token_url),
        }
    }
}

// API response models
#[derive(Debug, Clone, Deserialize)]
pub struct Community {
    pub id: Uuid,
    pub yardi_org_id: Option<String>,
    pub yardi_api_key: Option<String>,
    pub yardi_api_base_url: Option<String>,
    pub yardi_token_url: Option<String>,
}

fn mock_yardi_urls() -> (String, String) {
    let api_base_url =
        env::var("MOCK_YARDI_API_BASE_URL").expect("MOCK_YARDI_API_BASE_URL must be set");
    let token_url = env::var("MOCK_YARDI_TOKEN_URL").expect("MOCK_YARDI_TOKEN_URL must be set");
    (api_base_url, token_url)
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Location {
    pub id: Uuid,
    pub name: String,
    pub location_type: String,
    pub yardi_reference_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resident {
    pub id: Uuid,
    pub location_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub yardi_reference_id: Option<String>,
    pub photo: Option<ResidentPhotoMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResidentPhotoMetadata {
    pub etag: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: String,
}
