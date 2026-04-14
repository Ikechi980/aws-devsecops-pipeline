use std::net::IpAddr;

use anyhow::{Context, Result, anyhow};
use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, FromRequest, Request, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use tracing::{error, warn};
use x509_parser::certification_request::X509CertificationRequest;
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::prelude::FromDer;

use crate::community::ip_allowed;
use crate::error::AppError;
use crate::settings;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct SignRequest {
    csr: String,
}

#[derive(serde::Serialize)]
struct SignResponse {
    chain: Vec<String>,
}

pub struct LenientJson<T>(T);

impl<S, T> FromRequest<S> for LenientJson<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // Use Bytes::from_request which respects RequestBodyLimitLayer
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;

        let value = serde_json::from_slice(&bytes).map_err(|e| {
            warn!("invalid JSON body: {e:?}");
            AppError::BadRequest("Invalid JSON body".to_string())
        })?;

        Ok(LenientJson(value))
    }
}

pub async fn handler(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    State(state): State<AppState>,
    headers: HeaderMap,
    LenientJson(req): LenientJson<SignRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = settings::get();

    let xff = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let requester_ip = match extract_ip(Some(addr), xff) {
        Some(ip) => ip,
        None => {
            return Err(AppError::Forbidden("Missing requester IP".to_string()));
        }
    };

    let parsed = match parse_csr(&req.csr) {
        Ok(p) => p,
        Err(e) => {
            warn!("invalid CSR: {e:?}");
            return Err(AppError::BadRequest(
                "CSR must be a valid PEM encoded certificate request".to_string(),
            ));
        }
    };

    if !parsed.common_name.ends_with(&cfg.domain_suffix) {
        return Err(AppError::BadRequest(
            "CN must end with .ensurelink.net".to_string(),
        ));
    }

    let community_id = parsed
        .common_name
        .trim_end_matches(&cfg.domain_suffix)
        .trim_end_matches('.')
        .to_string();
    if community_id.is_empty() {
        return Err(AppError::BadRequest(
            "CN must be <community>.ensurelink.net".to_string(),
        ));
    }

    let bypass = ip_allowed(requester_ip);

    if !parsed
        .dns_names
        .iter()
        .all(|dns| dns.ends_with(&cfg.domain_suffix))
    {
        return Err(AppError::BadRequest(
            "All SAN entries must end with .ensurelink.net".to_string(),
        ));
    }

    if !bypass
        && parsed
            .dns_names
            .iter()
            .any(|dns| !dns.starts_with(&community_id))
    {
        return Err(AppError::Forbidden(
            "SANs must match the requesting community".to_string(),
        ));
    }

    if !bypass {
        match state.lookup.get_network_ip(&community_id).await {
            Ok(Some(expected)) if expected != requester_ip => {
                return Err(AppError::Forbidden(
                    "Unauthorized for requested community".to_string(),
                ));
            }
            Ok(None) => {
                return Err(AppError::NotFound("Community not found".to_string()));
            }
            Err(e) => {
                error!("community lookup failed: {e}");
                return Err(AppError::BadGateway(
                    "Authorization service error".to_string(),
                ));
            }
            _ => {}
        }
    }

    let signed = match state
        .ca_client
        .sign_certificate(&req.csr, &parsed.der, &parsed.common_name)
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            error!("signing failed: {e}");
            return Err(AppError::BadGateway(
                "Certificate authority error".to_string(),
            ));
        }
    };

    let reply = SignResponse {
        chain: signed.chain,
    };

    Ok((StatusCode::CREATED, Json(reply)))
}

fn extract_ip(remote: Option<std::net::SocketAddr>, xff: Option<String>) -> Option<IpAddr> {
    xff.as_deref()
        .and_then(|raw| raw.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
        .or_else(|| remote.map(|s| s.ip()))
}

struct ParsedCsr {
    common_name: String,
    dns_names: Vec<String>,
    der: Vec<u8>,
}

fn parse_csr(csr_pem: &str) -> Result<ParsedCsr> {
    let pem = pem::parse(csr_pem.trim()).context("parsing CSR PEM")?;
    if pem.tag() != "CERTIFICATE REQUEST" && pem.tag() != "NEW CERTIFICATE REQUEST" {
        return Err(anyhow!("unexpected PEM label {}", pem.tag()));
    }

    let der = pem.contents().to_vec();
    let (_, csr) = X509CertificationRequest::from_der(&der)
        .map_err(|_| anyhow!("failed to parse CSR body"))?;
    let common_name = csr
        .certification_request_info
        .subject
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("CSR is missing common name"))?;

    let mut dns_names = Vec::new();
    if let Some(exts) = csr.requested_extensions() {
        for ext in exts {
            if let ParsedExtension::SubjectAlternativeName(san) = ext {
                for name in san.general_names.iter() {
                    if let GeneralName::DNSName(dns) = name {
                        dns_names.push(dns.to_string());
                    }
                }
            }
        }
    }

    Ok(ParsedCsr {
        common_name,
        dns_names,
        der,
    })
}
