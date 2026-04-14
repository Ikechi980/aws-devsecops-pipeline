#![allow(dead_code)]

use std::path::PathBuf;

use reqwest::Client;
use tokio_tungstenite::Connector;

pub const BASE_URL: &str = "http://localhost:3000";
pub const WS_URL: &str = "wss://localhost:8443/gateway/v1/ws";
pub const HEALTH_URL: &str = "http://localhost:3000/v1/health";
pub const PKI_CERT_URL: &str = "http://localhost:8080/v1/certificates";

pub fn http_client() -> Client {
    Client::new()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Missing repo root")
        .to_path_buf()
}

fn ca_cert_path() -> PathBuf {
    repo_root()
        .join("infra")
        .join("stepca")
        .join("data")
        .join("certs")
        .join("root_ca.crt")
}

fn server_cert_path() -> PathBuf {
    repo_root()
        .join("infra")
        .join("nginx")
        .join("certs")
        .join("server.crt")
}

pub fn device_cn(device_name: &str) -> String {
    format!("{device_name}.ensurelink.net")
}

/// Load certificates for mTLS WebSocket connections
pub async fn load_tls_connector(device_name: &str) -> Connector {
    use std::fs;

    let (client_cert, client_key) = issue_client_cert(device_name)
        .await
        .expect("Failed to issue client cert from PKI");

    // Load CA certificate (client trust)
    let ca_cert = fs::read(ca_cert_path()).expect("Failed to read root CA cert");
    let server_cert = fs::read(server_cert_path()).expect("Failed to read nginx server cert");

    // Build native-tls client config with mTLS
    let identity = native_tls::Identity::from_pkcs8(client_cert.as_bytes(), client_key.as_bytes())
        .expect("Failed to create identity");

    let ca = native_tls::Certificate::from_pem(&ca_cert).expect("Failed to parse CA cert");
    let server_ca =
        native_tls::Certificate::from_pem(&server_cert).expect("Failed to parse server cert");

    let tls = native_tls::TlsConnector::builder()
        .identity(identity)
        .add_root_certificate(ca)
        .add_root_certificate(server_ca)
        .build()
        .expect("Failed to build TLS connector");

    Connector::NativeTls(tls)
}

async fn issue_client_cert(
    device_name: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let device_cn = device_cn(device_name);
    let key_pair = rcgen::KeyPair::generate()?;
    let mut params = rcgen::CertificateParams::new(vec![device_cn.clone()])?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, device_cn);
    let csr = params.serialize_request(&key_pair)?;
    let csr_pem = csr.pem()?;
    let key_pem = key_pair.serialize_pem();

    let response = http_client()
        .post(PKI_CERT_URL)
        .header("x-forwarded-for", "127.0.0.1")
        .json(&serde_json::json!({ "csr": csr_pem }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("PKI returned {status}: {body}").into());
    }

    #[derive(serde::Deserialize)]
    struct SignResponse {
        chain: Vec<String>,
    }

    let payload: SignResponse = response.json().await?;
    let cert_pem = payload.chain.join("");

    Ok((cert_pem, key_pem))
}

/// Publish a test message to LocalStack SNS
pub async fn publish_sns_message(
    target_community_id: &str,
    message_type: &str,
    versions: Vec<(u32, serde_json::Value)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let aws_endpoint =
        std::env::var("AWS_ENDPOINT_URL").unwrap_or_else(|_| "http://localhost:4666".to_string());

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(aws_endpoint)
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_sns::config::Credentials::new(
            "test", "test", None, None, "test",
        ))
        .load()
        .await;

    let sns_client = aws_sdk_sns::Client::new(&config);

    let versions_payload: Vec<_> = versions
        .into_iter()
        .map(|(version, payload)| {
            serde_json::json!({
                "version": version,
                "payload": payload
            })
        })
        .collect();

    let message = serde_json::json!({
        "target_community_id": target_community_id,
        "message_type": message_type,
        "versions": versions_payload
    });

    sns_client
        .publish()
        .topic_arn("arn:aws:sns:us-east-1:000000000000:headend-messages")
        .message(message.to_string())
        .send()
        .await?;

    Ok(())
}
