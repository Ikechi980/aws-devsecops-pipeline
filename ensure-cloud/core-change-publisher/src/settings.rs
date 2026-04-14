use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use std::env;

#[derive(Debug, Clone)]
pub struct Settings {
    pub systems_api_base_url: String,
    pub headend_sns_topic_arn: String,
    pub aws_endpoint_url: Option<String>,
}

static SETTINGS: OnceCell<Settings> = OnceCell::new();

impl Settings {
    pub fn init_from_env() -> Result<()> {
        let systems_api_base_url = get_required_env("SYSTEMS_API_BASE_URL")?;
        let headend_sns_topic_arn = get_required_env("HEADEND_SNS_TOPIC_ARN")?;
        let aws_endpoint_url = env::var("AWS_ENDPOINT_URL").ok();

        let settings = Settings {
            systems_api_base_url,
            headend_sns_topic_arn,
            aws_endpoint_url,
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

pub fn get_required_env(name: &str) -> Result<String> {
    match env::var(name) {
        Ok(v) => Ok(v),
        Err(env::VarError::NotPresent) => Err(anyhow!("Missing environment variable {name}")),
        Err(env::VarError::NotUnicode(_)) => Err(anyhow!(
            "Environment variable {name} contains invalid unicode"
        )),
    }
}
