use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use std::env;

#[derive(Debug, Clone)]
pub struct Settings {
    pub change_log_table_name: String,
    pub aws_endpoint_url: Option<String>,
}

static SETTINGS: OnceCell<Settings> = OnceCell::new();

impl Settings {
    pub fn init_from_env() -> Result<()> {
        let change_log_table_name = get_required_env("CHANGE_LOG_TABLE_NAME")?;
        let aws_endpoint_url = env::var("AWS_ENDPOINT_URL").ok();

        let settings = Settings {
            change_log_table_name,
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
