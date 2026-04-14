use std::env;
use std::sync::OnceLock;

use anyhow::{Context, Result};

static SETTINGS: OnceLock<Settings> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Settings {
    pub resources_api_base_url: String,
    pub yardi_poll_interval_ms: u64,
    pub resources_refresh_interval_secs: u64,
    pub aws_region: String,
    pub aws_endpoint_url: Option<String>,
    pub resources_events_queue_url: String,
    pub failure_sns_topic_arn: String,
}

impl Settings {
    pub fn init_from_env() -> Result<()> {
        let settings = Self {
            resources_api_base_url: required_env("RESOURCES_API_BASE_URL")?,
            yardi_poll_interval_ms: required_env("YARDI_POLL_INTERVAL_MS")?
                .parse()
                .context("YARDI_POLL_INTERVAL_MS must be a positive integer")?,
            resources_refresh_interval_secs: required_env("RESOURCES_REFRESH_INTERVAL_SECS")?
                .parse()
                .context("RESOURCES_REFRESH_INTERVAL_SECS must be a positive integer")?,
            aws_region: required_env("AWS_REGION")?,
            aws_endpoint_url: env::var("AWS_ENDPOINT_URL").ok(),
            resources_events_queue_url: required_env("RESOURCES_EVENTS_QUEUE_URL")?,
            failure_sns_topic_arn: required_env("FAILURE_SNS_TOPIC_ARN")?,
        };

        SETTINGS
            .set(settings)
            .expect("Settings already initialized");
        Ok(())
    }
}

pub fn get() -> &'static Settings {
    SETTINGS.get().expect("Settings not initialized")
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} must be set"))
}
