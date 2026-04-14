use crate::error::AppError;
use reqwest::StatusCode;
use serde::Deserialize;

#[derive(Clone)]
pub struct SystemsClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct EnsureSystem {
    #[serde(rename = "coreCommunityId")]
    core_community_id: Option<String>,
}

impl SystemsClient {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self { client, base_url }
    }

    pub async fn get_core_community_id(
        &self,
        ensure_community_id: &str,
    ) -> Result<Option<String>, AppError> {
        let url = format!(
            "{}/api/v1/ensure-systems/{}",
            self.base_url.trim_end_matches('/'),
            ensure_community_id
        );

        let response = self.client.get(url).send().await.map_err(|err| {
            tracing::error!(error = %err, "Failed to call ensure360-ems");
            AppError::bad_gateway(
                "ensure360_ems_unavailable",
                "Ensure systems API is unavailable",
            )
        })?;

        match response.status() {
            StatusCode::OK => {
                let system: EnsureSystem = response.json().await.map_err(|err| {
                    tracing::error!(error = %err, "Failed to parse ensure360-ems response");
                    AppError::bad_gateway(
                        "ensure360_ems_invalid_response",
                        "Ensure systems API returned an invalid response",
                    )
                })?;
                Ok(system.core_community_id)
            }
            StatusCode::NOT_FOUND => Ok(None),
            status => {
                tracing::warn!(status = %status, "Ensure systems API returned error status");
                Err(AppError::bad_gateway(
                    "ensure360_ems_error",
                    "Ensure systems API returned an error",
                ))
            }
        }
    }
}
