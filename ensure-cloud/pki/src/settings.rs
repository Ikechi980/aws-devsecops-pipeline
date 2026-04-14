use anyhow::{Result, anyhow};
use ipnet::IpNet;
use once_cell::sync::OnceCell;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

pub struct Settings {
    pub addr: SocketAddr,
    pub allowed_cidrs: Vec<IpNet>,
    pub system_api_base: String,
    pub ca_url: String,
    pub ca_certs_dir: PathBuf,
    pub provisioner_name: String,
    pub provisioner_key_id: String,
    pub provisioner_key_path: PathBuf,
    pub token_ttl: Duration,
    pub domain_suffix: String,
}

const DOMAIN_SUFFIX: &str = ".ensurelink.net";

static SETTINGS: OnceCell<Settings> = OnceCell::new();

impl Settings {
    pub fn init_from_env() -> Result<()> {
        let host = get_required_env("HOST")?;
        let port = get_required_env("PORT")?;
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|_| anyhow!("invalid HOST/PORT combination"))?;

        let cidrs_raw = get_required_env("ALLOWED_CIDRS")?;
        let allowed_cidrs: Vec<IpNet> = cidrs_raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(IpNet::from_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| anyhow!("ALLOWED_CIDRS contains an invalid CIDR"))?;

        if allowed_cidrs.is_empty() {
            return Err(anyhow!("ALLOWED_CIDRS must contain at least one CIDR"));
        }

        let system_api_base = get_required_env("SYSTEM_API_BASE")?;

        let ca_url = get_required_env("STEP_CA_URL")?;
        let ca_certs_dir = PathBuf::from(get_required_env("STEP_CA_CERTS_DIR")?);
        let provisioner_name = get_required_env("STEP_CA_PROVISIONER_NAME")?;
        let provisioner_key_id = get_required_env("STEP_CA_PROVISIONER_KEY_ID")?;
        let provisioner_key_path = PathBuf::from(get_required_env("STEP_CA_PROVISIONER_KEY_PATH")?);

        let token_ttl_str = get_required_env("STEP_CA_TOKEN_TTL_SECS")?;
        let token_ttl_secs = token_ttl_str
            .parse::<u64>()
            .map_err(|_| anyhow!("STEP_CA_TOKEN_TTL_SECS must be a valid number"))?;
        let token_ttl = Duration::from_secs(token_ttl_secs);

        let domain_suffix = DOMAIN_SUFFIX.to_string();

        let s = Settings {
            addr,
            allowed_cidrs,
            system_api_base,
            ca_url,
            ca_certs_dir,
            provisioner_name,
            provisioner_key_id,
            provisioner_key_path,
            token_ttl,
            domain_suffix,
        };

        SETTINGS
            .set(s)
            .map_err(|_| anyhow!("Settings already initialized"))?;

        Ok(())
    }
}

pub fn get() -> &'static Settings {
    SETTINGS.get().expect("Settings not initialized")
}

fn get_required_env(name: &str) -> Result<String> {
    match env::var(name) {
        Ok(v) => Ok(v),
        Err(env::VarError::NotPresent) => Err(anyhow!("Missing environment variable {name}")),
        Err(env::VarError::NotUnicode(_)) => Err(anyhow!(
            "Environment variable {name} contains invalid unicode"
        )),
    }
}
