use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::pkcs8::EncodePrivateKey;
use reqwest::Certificate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::settings::Settings;

#[async_trait]
pub trait CaClient: Send + Sync {
    async fn sign_certificate(
        &self,
        csr_pem: &str,
        csr_der: &[u8],
        subject: &str,
    ) -> Result<IssuedCertificate>;
}

pub struct StepCaClient {
    client: reqwest::Client,
    ca_url: String,
    provisioner_name: String,
    provisioner_key_id: String,
    encoding_key: EncodingKey,
    algorithm: Algorithm,
    token_ttl: Duration,
    root_cert_pem: String,
    intermediate_cert_pem: String,
}

impl StepCaClient {
    pub fn from_settings(cfg: &Settings) -> Result<Self> {
        let key_bytes = fs::read(&cfg.provisioner_key_path).with_context(|| {
            format!(
                "reading provisioner key at {}",
                cfg.provisioner_key_path.display()
            )
        })?;

        // Prefer EC keys (Smallstep defaults to ES256), fall back to RSA, then JWK JSON.
        let (encoding_key, algorithm) = match EncodingKey::from_ec_pem(&key_bytes) {
            Ok(key) => (key, Algorithm::ES256),
            Err(_) => match EncodingKey::from_rsa_pem(&key_bytes) {
                Ok(key) => (key, Algorithm::RS256),
                Err(_) => {
                    let key_str = std::str::from_utf8(&key_bytes)
                        .context("provisioner key is neither PEM nor UTF-8 JWK")?;
                    encoding_key_from_jwk(key_str)?
                }
            },
        };

        let root_cert_path = cfg.ca_certs_dir.join("root_ca.crt");
        let intermediate_cert_path = cfg.ca_certs_dir.join("intermediate_ca.crt");

        let mut client_builder = reqwest::Client::builder();
        let root_cert_pem = fs::read_to_string(&root_cert_path)
            .with_context(|| format!("reading CA root certificate {}", root_cert_path.display()))?;
        let cert = Certificate::from_pem(root_cert_pem.as_bytes())
            .context("parsing CA root certificate")?;
        client_builder = client_builder.add_root_certificate(cert);

        let intermediate_cert_pem =
            fs::read_to_string(&intermediate_cert_path).with_context(|| {
                format!(
                    "reading CA intermediate certificate {}",
                    intermediate_cert_path.display()
                )
            })?;

        let client = client_builder
            .use_rustls_tls()
            .build()
            .context("building CA HTTP client")?;

        Ok(Self {
            client,
            ca_url: cfg.ca_url.trim_end_matches('/').to_string(),
            provisioner_name: cfg.provisioner_name.clone(),
            provisioner_key_id: cfg.provisioner_key_id.clone(),
            encoding_key,
            algorithm,
            token_ttl: cfg.token_ttl,
            root_cert_pem,
            intermediate_cert_pem,
        })
    }

    fn build_token(&self, csr_der: &[u8], subject: &str) -> Result<String> {
        #[derive(Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            sub: &'a str,
            aud: String,
            nbf: i64,
            exp: i64,
            jti: String,
            sha: String,
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow!("system clock drift"))?
            .as_secs() as i64;
        let exp = now + self.token_ttl.as_secs() as i64;
        let sha = hex::encode(Sha256::digest(csr_der));

        let claims = Claims {
            iss: &self.provisioner_name,
            sub: subject,
            aud: format!("{}/1.0/sign", self.ca_url),
            nbf: now.saturating_sub(60),
            exp,
            jti: Uuid::new_v4().to_string(),
            sha,
        };

        let mut header = Header::new(self.algorithm);
        header.kid = Some(self.provisioner_key_id.clone());

        encode(&header, &claims, &self.encoding_key).context("building provisioning token")
    }
}

#[async_trait]
impl CaClient for StepCaClient {
    async fn sign_certificate(
        &self,
        csr_pem: &str,
        csr_der: &[u8],
        subject: &str,
    ) -> Result<IssuedCertificate> {
        let token = self.build_token(csr_der, subject)?;
        let url = format!("{}/1.0/sign", self.ca_url);

        let payload = SignRequest {
            csr: csr_pem,
            ott: &token,
        };

        let res = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .await
            .context("sending CSR to step-ca")?;

        let status = res.status();

        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(anyhow!(
                "step-ca sign failed with status {}: {}",
                status,
                body
            ));
        }

        let signed: SignResponse = res.json().await.context("parsing sign response")?;

        let issued_cert = signed
            .certificate
            .or(signed.crt)
            .ok_or_else(|| anyhow!("step-ca response missing certificate"))?;

        let chain = vec![
            issued_cert,
            self.intermediate_cert_pem.clone(),
            self.root_cert_pem.clone(),
        ];

        Ok(IssuedCertificate { chain })
    }
}

#[derive(Debug, Serialize)]
pub struct IssuedCertificate {
    pub chain: Vec<String>,
}

#[derive(Serialize)]
struct SignRequest<'a> {
    csr: &'a str,
    ott: &'a str,
}

#[derive(Deserialize)]
struct SignResponse {
    #[serde(default)]
    crt: Option<String>,
    #[serde(default)]
    certificate: Option<String>,
}

fn encoding_key_from_jwk(jwk_json: &str) -> Result<(EncodingKey, Algorithm)> {
    #[derive(Deserialize)]
    struct EcJwk<'a> {
        #[serde(rename = "kty")]
        key_type: &'a str,
        #[serde(rename = "crv")]
        curve: &'a str,
        #[serde(rename = "d")]
        private: &'a str,
    }

    let jwk: EcJwk = serde_json::from_str(jwk_json)
        .context("provisioner key is not valid PEM or EC JWK JSON")?;

    if jwk.key_type != "EC" {
        return Err(anyhow!(
            "unsupported JWK key type {} (only EC P-256 is supported)",
            jwk.key_type
        ));
    }
    if jwk.curve != "P-256" {
        return Err(anyhow!(
            "unsupported EC curve {} in JWK (expected P-256)",
            jwk.curve
        ));
    }

    let priv_bytes = URL_SAFE_NO_PAD
        .decode(jwk.private)
        .context("decoding JWK private key material")?;
    let secret = p256::SecretKey::from_slice(&priv_bytes)
        .map_err(|e| anyhow!("building EC key from JWK: {e}"))?;
    let pkcs8 = secret
        .to_pkcs8_der()
        .map_err(|e| anyhow!("encoding EC key to PKCS#8: {e}"))?;
    Ok((EncodingKey::from_ec_der(pkcs8.as_bytes()), Algorithm::ES256))
}
