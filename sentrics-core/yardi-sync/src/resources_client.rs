use anyhow::{Context, Result};
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use std::time::SystemTime;
use uuid::Uuid;

use crate::models::{Community, Location, Resident};
use crate::settings;

const RESOURCES_API_SIGNING_NAME: &str = "execute-api";

#[derive(Debug)]
pub enum ResourcesApiError {
    NotFound { reason: Option<String> },
    Conflict { reason: Option<String> },
    BadRequest { reason: Option<String> },
    Other(anyhow::Error),
}

impl std::fmt::Display for ResourcesApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourcesApiError::NotFound { reason } => {
                write!(f, "Resource not found: {:?}", reason)
            }
            ResourcesApiError::Conflict { reason } => write!(f, "Resource conflict: {:?}", reason),
            ResourcesApiError::BadRequest { reason } => write!(f, "Bad request: {:?}", reason),
            ResourcesApiError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ResourcesApiError {}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    reason: Option<String>,
}

pub struct ResourcesApiClient {
    http: reqwest::Client,
    base_url: String,
    signer: SigV4Signer,
}

impl ResourcesApiClient {
    pub fn new(http: reqwest::Client, aws_config: &aws_config::SdkConfig) -> Result<Self> {
        let cfg = settings::get();
        let signer = SigV4Signer::from_sdk_config(aws_config)?;
        Ok(Self {
            http,
            base_url: cfg.resources_api_base_url.clone(),
            signer,
        })
    }

    async fn signed_request(
        &self,
        method: reqwest::Method,
        url: String,
        body: Option<Vec<u8>>,
        context: &'static str,
    ) -> Result<reqwest::Response, ResourcesApiError> {
        let body_bytes = body.unwrap_or_default();
        let signing_headers = self
            .signer
            .signing_headers(method.as_str(), &url, &body_bytes)
            .await
            .context("Failed to sign resources-api request")
            .map_err(ResourcesApiError::Other)?;

        let mut request = self.http.request(method, &url);
        if !body_bytes.is_empty() {
            request = request.header(CONTENT_TYPE, "application/json");
            request = request.body(body_bytes);
        }

        for (name, value) in signing_headers.iter() {
            request = request.header(name.clone(), value.clone());
        }

        request
            .send()
            .await
            .context(context)
            .map_err(ResourcesApiError::Other)
    }

    async fn signed_request_with_headers(
        &self,
        method: reqwest::Method,
        url: String,
        body: Option<Vec<u8>>,
        extra_headers: Vec<(reqwest::header::HeaderName, String)>,
        context: &'static str,
    ) -> Result<reqwest::Response, ResourcesApiError> {
        let body_bytes = body.unwrap_or_default();
        let signing_headers = self
            .signer
            .signing_headers(method.as_str(), &url, &body_bytes)
            .await
            .context("Failed to sign resources-api request")
            .map_err(ResourcesApiError::Other)?;

        let mut request = self.http.request(method, &url);
        if !body_bytes.is_empty() {
            request = request.body(body_bytes);
        }

        for (name, value) in extra_headers {
            request = request.header(name, value);
        }

        for (name, value) in signing_headers.iter() {
            request = request.header(name.clone(), value.clone());
        }

        request
            .send()
            .await
            .context(context)
            .map_err(ResourcesApiError::Other)
    }

    async fn map_error(&self, response: reqwest::Response, context: String) -> ResourcesApiError {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let reason = serde_json::from_str::<ErrorBody>(&body)
            .ok()
            .and_then(|parsed| parsed.reason);

        match status {
            reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::PAYLOAD_TOO_LARGE => {
                ResourcesApiError::BadRequest { reason }
            }
            reqwest::StatusCode::NOT_FOUND => ResourcesApiError::NotFound { reason },
            reqwest::StatusCode::CONFLICT => ResourcesApiError::Conflict { reason },
            _ => ResourcesApiError::Other(anyhow::anyhow!("{}: {} - {}", context, status, body)),
        }
    }

    pub async fn list_communities(&self) -> Result<Vec<Community>, ResourcesApiError> {
        let url = format!("{}/communities", self.base_url);
        let response = self
            .signed_request(
                reqwest::Method::GET,
                url,
                None,
                "Failed to fetch communities from resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(response, "Failed to list communities".to_string())
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse communities response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn list_locations(
        &self,
        community_id: Uuid,
    ) -> Result<Vec<Location>, ResourcesApiError> {
        let url = format!("{}/communities/{}/locations", self.base_url, community_id);
        let response = self
            .signed_request(
                reqwest::Method::GET,
                url,
                None,
                "Failed to fetch locations from resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!("Failed to list locations for community {}", community_id),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse locations response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn list_residents(
        &self,
        community_id: Uuid,
    ) -> Result<Vec<Resident>, ResourcesApiError> {
        let url = format!("{}/communities/{}/residents", self.base_url, community_id);
        let response = self
            .signed_request(
                reqwest::Method::GET,
                url,
                None,
                "Failed to fetch residents from resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!("Failed to list residents for community {}", community_id),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse residents response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn create_location(
        &self,
        community_id: Uuid,
        name: &str,
        yardi_reference_id: &str,
    ) -> Result<Location, ResourcesApiError> {
        let url = format!("{}/communities/{}/locations", self.base_url, community_id);
        let body = serde_json::json!({
            "name": name,
            "location_type": "apartment",
            "yardi_reference_id": yardi_reference_id
        });

        let response = self
            .signed_request(
                reqwest::Method::POST,
                url,
                Some(
                    serde_json::to_vec(&body)
                        .context("Failed to serialize location create request")
                        .map_err(ResourcesApiError::Other)?,
                ),
                "Failed to create location in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!("Failed to create location for community {}", community_id),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse location response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn update_location(
        &self,
        community_id: Uuid,
        location_id: Uuid,
        name: &str,
        yardi_reference_id: Option<&str>,
    ) -> Result<Location, ResourcesApiError> {
        let url = format!(
            "{}/communities/{}/locations/{}",
            self.base_url, community_id, location_id
        );
        let body = serde_json::json!({
            "name": name,
            "location_type": "apartment",
            "yardi_reference_id": yardi_reference_id
        });

        let response = self
            .signed_request(
                reqwest::Method::PUT,
                url,
                Some(
                    serde_json::to_vec(&body)
                        .context("Failed to serialize location update request")
                        .map_err(ResourcesApiError::Other)?,
                ),
                "Failed to update location in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!(
                        "Failed to update location {} for community {}",
                        location_id, community_id
                    ),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse location response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn delete_location(
        &self,
        community_id: Uuid,
        location_id: Uuid,
    ) -> Result<(), ResourcesApiError> {
        let url = format!(
            "{}/communities/{}/locations/{}",
            self.base_url, community_id, location_id
        );

        let response = self
            .signed_request(
                reqwest::Method::DELETE,
                url,
                None,
                "Failed to delete location in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!(
                        "Failed to delete location {} for community {}",
                        location_id, community_id
                    ),
                )
                .await);
        }

        Ok(())
    }

    pub async fn create_resident(
        &self,
        community_id: Uuid,
        location_id: Uuid,
        first_name: &str,
        last_name: &str,
        yardi_reference_id: &str,
    ) -> Result<Resident, ResourcesApiError> {
        let url = format!("{}/communities/{}/residents", self.base_url, community_id);
        let body = serde_json::json!({
            "first_name": first_name,
            "last_name": last_name,
            "location_id": location_id,
            "yardi_reference_id": yardi_reference_id
        });

        let response = self
            .signed_request(
                reqwest::Method::POST,
                url,
                Some(
                    serde_json::to_vec(&body)
                        .context("Failed to serialize resident create request")
                        .map_err(ResourcesApiError::Other)?,
                ),
                "Failed to create resident in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!("Failed to create resident for community {}", community_id),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse resident response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn update_resident(
        &self,
        community_id: Uuid,
        resident_id: Uuid,
        first_name: &str,
        last_name: &str,
        location_id: Uuid,
        yardi_reference_id: Option<&str>,
    ) -> Result<Resident, ResourcesApiError> {
        let url = format!(
            "{}/communities/{}/residents/{}",
            self.base_url, community_id, resident_id
        );
        let body = serde_json::json!({
            "first_name": first_name,
            "last_name": last_name,
            "location_id": location_id,
            "yardi_reference_id": yardi_reference_id
        });

        let response = self
            .signed_request(
                reqwest::Method::PUT,
                url,
                Some(
                    serde_json::to_vec(&body)
                        .context("Failed to serialize resident update request")
                        .map_err(ResourcesApiError::Other)?,
                ),
                "Failed to update resident in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!(
                        "Failed to update resident {} for community {}",
                        resident_id, community_id
                    ),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse resident response")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn delete_resident(
        &self,
        community_id: Uuid,
        resident_id: Uuid,
    ) -> Result<(), ResourcesApiError> {
        let url = format!(
            "{}/communities/{}/residents/{}",
            self.base_url, community_id, resident_id
        );

        let response = self
            .signed_request(
                reqwest::Method::DELETE,
                url,
                None,
                "Failed to delete resident in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!(
                        "Failed to delete resident {} for community {}",
                        resident_id, community_id
                    ),
                )
                .await);
        }

        Ok(())
    }

    pub async fn put_resident_photo(
        &self,
        community_id: Uuid,
        resident_id: Uuid,
        content_type: &str,
        photo_bytes: Vec<u8>,
    ) -> Result<Resident, ResourcesApiError> {
        let url = format!(
            "{}/communities/{}/residents/{}/photo",
            self.base_url, community_id, resident_id
        );

        let response = self
            .signed_request_with_headers(
                reqwest::Method::PUT,
                url,
                Some(photo_bytes),
                vec![(CONTENT_TYPE, content_type.to_string())],
                "Failed to upload resident photo in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!(
                        "Failed to upload resident photo {} for community {}",
                        resident_id, community_id
                    ),
                )
                .await);
        }

        response
            .json()
            .await
            .context("Failed to parse resident response after photo upload")
            .map_err(ResourcesApiError::Other)
    }

    pub async fn delete_resident_photo(
        &self,
        community_id: Uuid,
        resident_id: Uuid,
    ) -> Result<(), ResourcesApiError> {
        let url = format!(
            "{}/communities/{}/residents/{}/photo",
            self.base_url, community_id, resident_id
        );

        let response = self
            .signed_request(
                reqwest::Method::DELETE,
                url,
                None,
                "Failed to delete resident photo in resources-api",
            )
            .await?;

        if !response.status().is_success() {
            return Err(self
                .map_error(
                    response,
                    format!(
                        "Failed to delete resident photo {} for community {}",
                        resident_id, community_id
                    ),
                )
                .await);
        }

        Ok(())
    }
}

struct SigV4Signer {
    credentials_provider: SharedCredentialsProvider,
    region: String,
}

impl SigV4Signer {
    fn from_sdk_config(aws_config: &aws_config::SdkConfig) -> Result<Self> {
        let region = aws_config
            .region()
            .map(|region| region.as_ref().to_string())
            .context("AWS region must be configured for resources-api signing")?;
        let credentials_provider = aws_config
            .credentials_provider()
            .context("AWS credentials provider must be configured")?;

        Ok(Self {
            credentials_provider,
            region,
        })
    }

    async fn signing_headers(&self, method: &str, uri: &str, body: &[u8]) -> Result<HeaderMap> {
        let credentials = self
            .credentials_provider
            .provide_credentials()
            .await
            .context("Failed to load AWS credentials")?;
        let identity = credentials.into();
        let signing_params = v4::SigningParams::builder()
            .identity(&identity)
            .region(&self.region)
            .name(RESOURCES_API_SIGNING_NAME)
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()
            .context("Failed to build SigV4 signing params")?;
        let signable_request =
            SignableRequest::new(method, uri, std::iter::empty(), SignableBody::Bytes(body))
                .context("Failed to create signable request")?;
        let (instructions, _signature) = sign(signable_request, &signing_params.into())
            .context("Failed to sign request")?
            .into_parts();

        let mut headers = HeaderMap::new();
        for (name, value) in instructions.headers() {
            let name =
                HeaderName::from_bytes(name.as_bytes()).context("Invalid SigV4 header name")?;
            let value = HeaderValue::from_str(value).context("Invalid SigV4 header value")?;
            headers.insert(name, value);
        }

        Ok(headers)
    }
}
