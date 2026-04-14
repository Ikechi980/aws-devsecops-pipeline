use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const SYSTEMS_CACHE_TTL_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct SystemsClient {
    inner: Arc<SystemsClientInner>,
}

impl SystemsClient {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        Self {
            inner: Arc::new(SystemsClientInner {
                client,
                base_url,
                cache: Mutex::new(None),
            }),
        }
    }

    pub async fn find_by_core_community_id(
        &self,
        core_community_id: &str,
    ) -> Result<Option<EnsureSystem>> {
        let systems = self.get_cached_systems().await?;

        Ok(systems
            .into_iter()
            .find(|system| system.core_community_id.as_deref() == Some(core_community_id)))
    }

    async fn get_cached_systems(&self) -> Result<Vec<EnsureSystem>> {
        let mut cache = self.inner.cache.lock().await;
        if let Some(entry) = cache.as_ref()
            && entry.fetched_at.elapsed() < Duration::from_secs(SYSTEMS_CACHE_TTL_SECS)
        {
            return Ok(entry.systems.clone());
        }

        let systems = self.fetch_systems().await?;
        *cache = Some(SystemsCache {
            fetched_at: Instant::now(),
            systems: systems.clone(),
        });

        Ok(systems)
    }

    async fn fetch_systems(&self) -> Result<Vec<EnsureSystem>> {
        let url = format!(
            "{}/api/v1/ensure-systems",
            self.inner.base_url.trim_end_matches('/')
        );
        let response = self.inner.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("systems api error: status {}", response.status()));
        }

        let systems: Vec<EnsureSystem> = response.json().await?;
        Ok(systems)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnsureSystem {
    #[serde(rename = "communityId")]
    pub community_id: String,
    #[serde(rename = "coreCommunityId")]
    pub core_community_id: Option<String>,
}

#[derive(Clone, Debug)]
struct SystemsCache {
    fetched_at: Instant,
    systems: Vec<EnsureSystem>,
}

#[derive(Debug)]
struct SystemsClientInner {
    client: reqwest::Client,
    base_url: String,
    cache: Mutex<Option<SystemsCache>>,
}
