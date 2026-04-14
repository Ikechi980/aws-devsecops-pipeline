use std::net::IpAddr;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::StatusCode;

use crate::settings;

#[async_trait]
pub trait CommunityLookup: Send + Sync {
    async fn get_network_ip(&self, community_id: &str) -> Result<Option<IpAddr>>;
}

pub struct HttpCommunityLookup {
    client: reqwest::Client,
    base_url: String,
}

impl HttpCommunityLookup {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }
}

#[async_trait]
impl CommunityLookup for HttpCommunityLookup {
    async fn get_network_ip(&self, community_id: &str) -> Result<Option<IpAddr>> {
        let url = format!("{}{}", self.base_url, community_id);
        let res = self.client.get(url).send().await?;

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !res.status().is_success() {
            return Err(anyhow!("upstream error: status {}", res.status()));
        }

        #[derive(serde::Deserialize)]
        struct Resp {
            #[serde(rename = "networkIp")]
            network_ip: Option<String>,
        }

        let body: Resp = res.json().await?;
        Ok(body.network_ip.and_then(|s| s.parse().ok()))
    }
}

pub fn ip_allowed(ip: IpAddr) -> bool {
    let cfg = settings::get();
    cfg.allowed_cidrs.iter().any(|net| net.contains(&ip))
}
