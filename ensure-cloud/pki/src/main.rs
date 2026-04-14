use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::Url;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod ca_client;
mod community;
mod error;
mod routes;
mod settings;
mod state;

use ca_client::StepCaClient;
use community::HttpCommunityLookup;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    settings::Settings::init_from_env()?;
    let cfg = settings::get();
    wait_for_step_ca(&cfg.ca_url).await?;

    let lookup = Arc::new(HttpCommunityLookup::new(cfg.system_api_base.clone()));
    let ca_client = Arc::new(StepCaClient::from_settings(cfg)?) as Arc<_>;

    let app_state = state::AppState { lookup, ca_client };

    let app = routes::create_router(app_state);

    let addr = cfg.addr;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("serving on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn step_ca_addr(ca_url: &str) -> Result<(String, u16)> {
    let url = Url::parse(ca_url).map_err(|e| anyhow!("invalid STEP_CA_URL: {e}"))?;
    let scheme = url.scheme();

    if scheme != "http" && scheme != "https" {
        return Err(anyhow!(
            "STEP_CA_URL must use http or https (found scheme {scheme})"
        ));
    }

    let port = match (scheme, url.port()) {
        (_, Some(port)) => port,
        ("https", None) => 443,
        ("http", None) => 80,
        _ => unreachable!("unsupported schemes are rejected above"),
    };

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("STEP_CA_URL must include a host"))?
        .to_string();

    Ok((host, port))
}

async fn wait_for_step_ca(ca_url: &str) -> Result<()> {
    let (host, port) = step_ca_addr(ca_url)?;

    tracing::info!("waiting for step-ca at {}:{}", host, port);
    loop {
        match TcpStream::connect((host.as_str(), port)).await {
            Ok(_) => {
                tracing::info!("step-ca is available at {}:{}", host, port);
                return Ok(());
            }
            Err(_) => sleep(Duration::from_millis(500)).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::step_ca_addr;

    #[test]
    fn parses_https_with_explicit_port() {
        let (host, port) =
            step_ca_addr("https://step-ca.internal:9443").expect("expected valid URL");
        assert_eq!(host, "step-ca.internal");
        assert_eq!(port, 9443);
    }

    #[test]
    fn defaults_https_port() {
        let (host, port) = step_ca_addr("https://step-ca").expect("expected valid URL");
        assert_eq!(host, "step-ca");
        assert_eq!(port, 443);
    }

    #[test]
    fn defaults_http_port() {
        let (host, port) = step_ca_addr("http://step-ca").expect("expected valid URL");
        assert_eq!(host, "step-ca");
        assert_eq!(port, 80);
    }

    #[test]
    fn rejects_missing_host() {
        let err = step_ca_addr("https://").expect_err("expected invalid URL error");
        assert!(err.to_string().contains("invalid STEP_CA_URL"));
    }

    #[test]
    fn rejects_unsupported_scheme() {
        let err = step_ca_addr("tcp://step-ca:9000").expect_err("expected unsupported scheme");
        assert!(err.to_string().contains("must use http or https"));
    }
}
