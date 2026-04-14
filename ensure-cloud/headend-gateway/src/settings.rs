use std::net::SocketAddr;

use anyhow::{Context, Result, anyhow};
use once_cell::sync::OnceCell;

static SETTINGS: OnceCell<Settings> = OnceCell::new();

#[derive(Debug, Clone)]
pub struct Settings {
    pub addr: SocketAddr,
    pub aws_endpoint_url: Option<String>,
    pub sns_topic_arn: String,
}

impl Settings {
    pub fn init_from_env() -> Result<()> {
        let host = std::env::var("HOST").context("HOST must be set")?;
        let port: u16 = std::env::var("PORT")
            .context("PORT must be set")?
            .parse()
            .context("PORT must be a valid u16")?;

        let addr = format!("{host}:{port}")
            .parse()
            .context("Invalid HOST:PORT combination")?;

        let aws_endpoint_url = std::env::var("AWS_ENDPOINT_URL").ok();

        let sns_topic_arn =
            std::env::var("HEADEND_SNS_TOPIC_ARN").context("HEADEND_SNS_TOPIC_ARN must be set")?;

        let settings = Settings {
            addr,
            aws_endpoint_url,
            sns_topic_arn,
        };

        SETTINGS
            .set(settings)
            .map_err(|_| anyhow!("Settings already initialized"))?;

        Ok(())
    }
}

pub fn get() -> &'static Settings {
    SETTINGS.get().expect("Settings not initialized")
}
