use crate::{error::AppError, state::AppState};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};
use lambda_http::request::RequestContext;

#[derive(Debug, Clone)]
pub struct EnsureCommunity {
    pub ensure_community_id: String,
}

impl EnsureCommunity {
    fn from_subject_dn(subject_dn: &str) -> Option<Self> {
        let cn = extract_cn_from_subject_dn(subject_dn)?;
        let ensure_community_id = ensure_community_id_from_cn(&cn)?;
        Some(Self {
            ensure_community_id,
        })
    }
}

impl FromRequestParts<AppState> for EnsureCommunity {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(ensure_id) = local_override(&parts.headers, state)? {
            return Ok(EnsureCommunity {
                ensure_community_id: ensure_id,
            });
        }

        match parts.extensions.get::<RequestContext>() {
            Some(RequestContext::ApiGatewayV2(ctx)) => {
                let subject_dn = ctx
                    .authentication
                    .as_ref()
                    .and_then(|auth| auth.client_cert.as_ref())
                    .and_then(|cert| cert.subject_dn.as_deref())
                    .ok_or_else(|| {
                        AppError::internal_server_error(
                            "client_certificate_missing",
                            "Client certificate details were not provided by API Gateway",
                        )
                    })?;

                EnsureCommunity::from_subject_dn(subject_dn).ok_or_else(|| {
                    AppError::unauthorized(
                        "client_certificate_invalid",
                        "Client certificate subject is missing a valid CN",
                    )
                })
            }
            Some(RequestContext::ApiGatewayV1(_)) => Err(AppError::internal_server_error(
                "api_gateway_payload_version_unsupported",
                "API Gateway payload format version 2.0 is required to read client certificate details",
            )),
            Some(_) => Err(AppError::internal_server_error(
                "api_gateway_type_unsupported",
                "Unsupported API Gateway request context",
            )),
            None => Err(AppError::internal_server_error(
                "api_gateway_context_missing",
                "Missing API Gateway request context",
            )),
        }
    }
}

fn local_override(headers: &HeaderMap, state: &AppState) -> Result<Option<String>, AppError> {
    if !state.allow_unauthenticated {
        return Ok(None);
    }

    if let Some(value) = headers.get("x-ensure-community-id") {
        let ensure_id = value.to_str().map_err(|_| {
            AppError::bad_request(
                "ensure_community_id_invalid",
                "Invalid x-ensure-community-id header",
            )
        })?;
        if ensure_id.trim().is_empty() {
            return Err(AppError::bad_request(
                "ensure_community_id_missing",
                "x-ensure-community-id header is empty",
            ));
        }
        tracing::warn!(
            "ALLOW_UNAUTHENTICATED is set - using x-ensure-community-id for local development. This should never be enabled in production!"
        );
        return Ok(Some(ensure_id.trim().to_string()));
    }

    Err(AppError::bad_request(
        "ensure_community_id_missing",
        "Local development requires x-ensure-community-id",
    ))
}

fn extract_cn_from_subject_dn(subject_dn: &str) -> Option<String> {
    subject_dn
        .split(',')
        .flat_map(|segment| segment.split('/'))
        .map(|segment| segment.trim())
        .find_map(|segment| {
            if segment.len() >= 3 && segment[..3].eq_ignore_ascii_case("CN=") {
                Some(segment[3..].trim().to_string())
            } else {
                None
            }
        })
}

fn ensure_community_id_from_cn(cn: &str) -> Option<String> {
    let cn = cn.trim();
    let lower = cn.to_ascii_lowercase();
    let suffix = ".ensurelink.net";
    let id = lower.strip_suffix(suffix)?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core_resources::CoreResourcesClient, systems::SystemsClient};
    use axum::http::Request;
    use lambda_http::aws_lambda_events::apigw::{
        ApiGatewayProxyRequestContext, ApiGatewayV2httpRequestContext,
        ApiGatewayV2httpRequestContextAuthentication,
        ApiGatewayV2httpRequestContextAuthenticationClientCert,
        ApiGatewayV2httpRequestContextAuthenticationClientCertValidity,
    };

    #[test]
    fn parses_cn_from_subject_dn() {
        let subject = "CN=alpha.ensurelink.net,OU=Devices,O=Ensure";
        let cn = extract_cn_from_subject_dn(subject).unwrap();
        assert_eq!(cn, "alpha.ensurelink.net");
    }

    #[test]
    fn parses_cn_from_slash_subject_dn() {
        let subject = "/C=US/ST=MN/CN=beta.ensurelink.net";
        let cn = extract_cn_from_subject_dn(subject).unwrap();
        assert_eq!(cn, "beta.ensurelink.net");
    }

    #[test]
    fn rejects_subject_dn_without_cn() {
        let subject = "OU=Devices,O=Ensure";
        assert!(extract_cn_from_subject_dn(subject).is_none());
    }

    #[test]
    fn extracts_ensure_community_id_from_cn() {
        let ensure_id = ensure_community_id_from_cn("alpha.ensurelink.net").unwrap();
        assert_eq!(ensure_id, "alpha");
    }

    #[test]
    fn rejects_cn_without_suffix() {
        assert!(ensure_community_id_from_cn("alpha").is_none());
    }

    fn app_state(allow_unauthenticated: bool) -> AppState {
        use crate::events_repo::EventsRepo;
        use crate::models::GlobalEvent;
        use async_trait::async_trait;
        use aws_credential_types::Credentials;
        use std::sync::Arc;

        struct MockEventsRepo;
        #[async_trait]
        impl EventsRepo for MockEventsRepo {
            async fn fetch_events(
                &self,
                _community_id: &str,
                _payload_types: &[String],
                _after: Option<chrono::DateTime<chrono::Utc>>,
                _before: Option<chrono::DateTime<chrono::Utc>>,
                _limit: u32,
            ) -> anyhow::Result<Vec<GlobalEvent>> {
                Ok(vec![])
            }
        }

        let credentials = Credentials::new("test", "test", None, None, "test");
        let aws_config = aws_config::SdkConfig::builder()
            .credentials_provider(
                aws_credential_types::provider::SharedCredentialsProvider::new(credentials),
            )
            .region(aws_config::Region::new("us-east-1"))
            .build();

        let http_client = reqwest::Client::new();
        AppState {
            systems: SystemsClient::new(http_client.clone(), "http://example.com".to_string()),
            core_resources: CoreResourcesClient::new(
                http_client,
                "http://example.com".to_string(),
                &aws_config,
            )
            .expect("Failed to create CoreResourcesClient"),
            events_repo: Arc::new(MockEventsRepo),
            events_limit_default: 100,
            events_limit_max: 1000,
            allow_unauthenticated,
        }
    }

    fn v2_context_with_subject(subject_dn: Option<String>) -> RequestContext {
        let mut ctx = ApiGatewayV2httpRequestContext::default();
        let mut cert = ApiGatewayV2httpRequestContextAuthenticationClientCert::default();
        cert.subject_dn = subject_dn;
        cert.validity = ApiGatewayV2httpRequestContextAuthenticationClientCertValidity::default();

        let mut auth = ApiGatewayV2httpRequestContextAuthentication::default();
        auth.client_cert = Some(cert);

        ctx.authentication = Some(auth);
        RequestContext::ApiGatewayV2(ctx)
    }

    #[tokio::test]
    async fn rejects_missing_request_context() {
        let state = app_state(false);
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();

        let err = EnsureCommunity::from_request_parts(&mut parts, &state)
            .await
            .expect_err("expected error");

        match err {
            AppError::InternalServer { reason, .. } => {
                assert_eq!(reason, "api_gateway_context_missing");
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[tokio::test]
    async fn rejects_v1_request_context() {
        let state = app_state(false);
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        parts.extensions.insert(RequestContext::ApiGatewayV1(
            ApiGatewayProxyRequestContext::default(),
        ));

        let err = EnsureCommunity::from_request_parts(&mut parts, &state)
            .await
            .expect_err("expected error");

        match err {
            AppError::InternalServer { reason, .. } => {
                assert_eq!(reason, "api_gateway_payload_version_unsupported");
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[tokio::test]
    async fn rejects_missing_client_cert_details() {
        let state = app_state(false);
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        parts.extensions.insert(v2_context_with_subject(None));

        let err = EnsureCommunity::from_request_parts(&mut parts, &state)
            .await
            .expect_err("expected error");

        match err {
            AppError::InternalServer { reason, .. } => {
                assert_eq!(reason, "client_certificate_missing");
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_client_cert_cn() {
        let state = app_state(false);
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        parts
            .extensions
            .insert(v2_context_with_subject(Some("CN=alpha".to_string())));

        let err = EnsureCommunity::from_request_parts(&mut parts, &state)
            .await
            .expect_err("expected error");

        match err {
            AppError::Unauthorized { reason, .. } => {
                assert_eq!(reason, "client_certificate_invalid");
            }
            _ => panic!("unexpected error variant"),
        }
    }

    #[tokio::test]
    async fn accepts_valid_client_cert_cn() {
        let state = app_state(false);
        let req = Request::builder()
            .uri("/")
            .body(axum::body::Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        parts.extensions.insert(v2_context_with_subject(Some(
            "CN=alpha.ensurelink.net".to_string(),
        )));

        let ensure = EnsureCommunity::from_request_parts(&mut parts, &state)
            .await
            .expect("expected ensure community");

        assert_eq!(ensure.ensure_community_id, "alpha");
    }
}
