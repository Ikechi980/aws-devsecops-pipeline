use crate::error::AppError;
use crate::models::{Community, Location, Resident};
use anyhow::Context;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use reqwest::StatusCode;
use reqwest::header::{
    CONTENT_TYPE, ETAG, HeaderMap, HeaderName, HeaderValue, IF_NONE_MATCH, LAST_MODIFIED,
};
use std::time::SystemTime;
use uuid::Uuid;

const RESOURCES_API_SIGNING_NAME: &str = "execute-api";

pub struct ResidentPhoto {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub enum ResidentPhotoResponse {
    Ok(ResidentPhoto),
    NotModified { etag: Option<String> },
}

#[derive(Clone)]
pub struct CoreResourcesClient {
    client: reqwest::Client,
    base_url: String,
    signer: SigV4Signer,
}

impl CoreResourcesClient {
    pub fn new(
        client: reqwest::Client,
        base_url: String,
        aws_config: &aws_config::SdkConfig,
    ) -> Result<Self, anyhow::Error> {
        let signer = SigV4Signer::from_sdk_config(aws_config)?;
        Ok(Self {
            client,
            base_url,
            signer,
        })
    }

    pub async fn get_community(&self, core_community_id: &str) -> Result<Community, AppError> {
        self.get(&format!("/v1/communities/{}", core_community_id))
            .await
    }

    pub async fn get_locations(&self, core_community_id: &str) -> Result<Vec<Location>, AppError> {
        self.get(&format!("/v1/communities/{}/locations", core_community_id))
            .await
    }

    pub async fn get_residents(&self, core_community_id: &str) -> Result<Vec<Resident>, AppError> {
        self.get(&format!("/v1/communities/{}/residents", core_community_id))
            .await
    }

    pub async fn get_resident_photo(
        &self,
        core_community_id: &str,
        resident_id: Uuid,
        if_none_match: Option<HeaderValue>,
    ) -> Result<ResidentPhotoResponse, AppError> {
        let path = format!(
            "/v1/communities/{}/residents/{}/photo",
            core_community_id, resident_id
        );

        let mut extra_headers = HeaderMap::new();
        if let Some(value) = if_none_match {
            extra_headers.insert(IF_NONE_MATCH, value);
        }

        let response = self.send_get(&path, extra_headers).await?;

        match response.status() {
            StatusCode::OK => {
                let content_type = response
                    .headers()
                    .get(CONTENT_TYPE)
                    .ok_or_else(|| {
                        AppError::bad_gateway(
                            "core_resources_invalid_response",
                            "Core resources API returned an invalid response",
                        )
                    })?
                    .to_str()
                    .map_err(|_| {
                        AppError::bad_gateway(
                            "core_resources_invalid_response",
                            "Core resources API returned an invalid response",
                        )
                    })?
                    .to_string();

                let etag = response
                    .headers()
                    .get(ETAG)
                    .map(|value| {
                        value.to_str().map(str::to_string).map_err(|_| {
                            AppError::bad_gateway(
                                "core_resources_invalid_response",
                                "Core resources API returned an invalid response",
                            )
                        })
                    })
                    .transpose()?;

                let last_modified = response
                    .headers()
                    .get(LAST_MODIFIED)
                    .map(|value| {
                        value.to_str().map(str::to_string).map_err(|_| {
                            AppError::bad_gateway(
                                "core_resources_invalid_response",
                                "Core resources API returned an invalid response",
                            )
                        })
                    })
                    .transpose()?;

                let bytes = response.bytes().await.map_err(|err| {
                    tracing::error!(error = %err, "Failed to read core resources photo response");
                    AppError::bad_gateway(
                        "core_resources_invalid_response",
                        "Core resources API returned an invalid response",
                    )
                })?;

                Ok(ResidentPhotoResponse::Ok(ResidentPhoto {
                    bytes: bytes.to_vec(),
                    content_type,
                    etag,
                    last_modified,
                }))
            }
            StatusCode::NOT_MODIFIED => {
                let etag = response
                    .headers()
                    .get(ETAG)
                    .map(|value| {
                        value.to_str().map(str::to_string).map_err(|_| {
                            AppError::bad_gateway(
                                "core_resources_invalid_response",
                                "Core resources API returned an invalid response",
                            )
                        })
                    })
                    .transpose()?;
                Ok(ResidentPhotoResponse::NotModified { etag })
            }
            StatusCode::NOT_FOUND => Err(AppError::not_found(
                "core_resource_not_found",
                "Core resource not found",
            )),
            status => {
                tracing::warn!(status = %status, "Core resources API returned error status");
                Err(AppError::bad_gateway(
                    "core_resources_error",
                    "Core resources API returned an error",
                ))
            }
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, AppError> {
        let response = self.send_get(path, HeaderMap::new()).await?;

        match response.status() {
            StatusCode::OK => response.json().await.map_err(|err| {
                tracing::error!(error = %err, "Failed to deserialize core resources response");
                AppError::bad_gateway(
                    "core_resources_invalid_response",
                    "Core resources API returned an invalid response",
                )
            }),
            StatusCode::NOT_FOUND => Err(AppError::not_found(
                "core_resource_not_found",
                "Core resource not found",
            )),
            status => {
                tracing::warn!(status = %status, "Core resources API returned error status");
                Err(AppError::bad_gateway(
                    "core_resources_error",
                    "Core resources API returned an error",
                ))
            }
        }
    }

    async fn send_get(
        &self,
        path: &str,
        extra_headers: HeaderMap,
    ) -> Result<reqwest::Response, AppError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);

        let signing_headers = self
            .signer
            .signing_headers("GET", &url, &[])
            .await
            .map_err(|err| {
                tracing::error!(error = %err, "Failed to sign request");
                AppError::internal_server_error(
                    "signing_failed",
                    "Failed to sign request to core resources API",
                )
            })?;

        let mut request = self.client.get(&url);
        for (name, value) in signing_headers.iter() {
            request = request.header(name.clone(), value.clone());
        }
        for (name, value) in extra_headers.iter() {
            request = request.header(name.clone(), value.clone());
        }

        request.send().await.map_err(|err| {
            tracing::error!(error = %err, "Failed to call core resources API");
            AppError::bad_gateway(
                "core_resources_unavailable",
                "Core resources API is unavailable",
            )
        })
    }
}

#[derive(Clone)]
struct SigV4Signer {
    credentials_provider: SharedCredentialsProvider,
    region: String,
}

impl SigV4Signer {
    fn from_sdk_config(aws_config: &aws_config::SdkConfig) -> Result<Self, anyhow::Error> {
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

    async fn signing_headers(
        &self,
        method: &str,
        uri: &str,
        body: &[u8],
    ) -> Result<HeaderMap, anyhow::Error> {
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
