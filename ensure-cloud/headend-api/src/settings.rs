use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use std::env;

#[derive(Debug, Clone)]
pub struct Settings {
    pub systems_api_base_url: String,
    pub core_resources_api_base_url: String,
    pub allow_unauthenticated: bool,
    pub events_mongo_url_ssm_parameter: String,
    pub events_limit_default: u32,
    pub events_limit_max: u32,
}

static SETTINGS: OnceCell<Settings> = OnceCell::new();

impl Settings {
    pub fn init_from_env() -> Result<()> {
        let systems_api_base_url = get_required_env("SYSTEMS_API_BASE_URL")?;
        let core_resources_api_base_url = get_required_env("CORE_RESOURCES_API_BASE_URL")?;
        let allow_unauthenticated = env::var("ALLOW_UNAUTHENTICATED").is_ok();
        let events_mongo_url_ssm_parameter = get_required_env("EVENTS_MONGO_URL_SSM_PARAMETER")?;
        let events_limit_default: u32 = get_required_env("EVENTS_LIMIT_DEFAULT")?
            .parse()
            .map_err(|_| anyhow!("EVENTS_LIMIT_DEFAULT must be a positive integer"))?;
        if events_limit_default == 0 {
            return Err(anyhow!("EVENTS_LIMIT_DEFAULT must be > 0"));
        }
        let events_limit_max: u32 = get_required_env("EVENTS_LIMIT_MAX")?
            .parse()
            .map_err(|_| anyhow!("EVENTS_LIMIT_MAX must be a positive integer"))?;
        if events_limit_max == 0 {
            return Err(anyhow!("EVENTS_LIMIT_MAX must be > 0"));
        }

        if events_limit_default > events_limit_max {
            return Err(anyhow!(
                "EVENTS_LIMIT_DEFAULT cannot exceed EVENTS_LIMIT_MAX"
            ));
        }

        let settings = Self {
            systems_api_base_url,
            core_resources_api_base_url,
            allow_unauthenticated,
            events_mongo_url_ssm_parameter,
            events_limit_default,
            events_limit_max,
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
