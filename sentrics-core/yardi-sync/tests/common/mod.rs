//! Common test utilities and infrastructure.

use std::env;
use std::future::Future;
use std::time::Duration;

pub mod aws;
pub mod mock_yardi;
pub mod resources;

pub use mock_yardi::{
    FailureConfig, MockEncounter, MockLocation, MockPatient, MockYardiClient, OrganizationConfig,
};
pub use resources::{CreateCommunity, ResourcesApiClient, UpdateCommunity};

/// Test context providing access to all test infrastructure.
pub struct TestContext {
    pub resources_api: ResourcesApiClient,
    pub mock_yardi: MockYardiClient,
}

impl TestContext {
    pub async fn new() -> Self {
        let _ = dotenvy::dotenv();

        let resources_api_base_url =
            env::var("RESOURCES_API_BASE_URL").expect("RESOURCES_API_BASE_URL must be set");

        let mock_yardi_base_url =
            env::var("MOCK_YARDI_API_BASE_URL").expect("MOCK_YARDI_API_BASE_URL must be set");

        Self {
            resources_api: ResourcesApiClient::new(&resources_api_base_url),
            mock_yardi: MockYardiClient::new(&mock_yardi_base_url),
        }
    }

    /// Waits for infrastructure to be ready.
    pub async fn wait_for_infrastructure(&self) -> anyhow::Result<()> {
        // Wait for resources-api
        for _ in 0..30 {
            if self.resources_api.health_check().await? {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Wait for mock-yardi-api
        for _ in 0..30 {
            if self.mock_yardi.health_check().await? {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Verify both are up
        if !self.resources_api.health_check().await? {
            anyhow::bail!("resources-api is not available");
        }
        if !self.mock_yardi.health_check().await? {
            anyhow::bail!("mock-yardi-api is not available");
        }

        Ok(())
    }

    /// Cleanup helper that deletes a community and ignores errors.
    pub async fn cleanup_community(&self, id: uuid::Uuid) {
        let _ = self.resources_api.delete_community(id).await;
    }
}

/// Generates a unique test name suffix.
pub fn unique_suffix() -> String {
    uuid::Uuid::new_v4().to_string()[..8].to_string()
}

/// How long to wait for a sync cycle to complete.
pub fn sync_wait_time() -> Duration {
    poll_interval()
}

/// Poll interval configured for yardi-sync.
pub fn poll_interval() -> Duration {
    let poll_interval_ms = env::var("YARDI_POLL_INTERVAL_MS")
        .expect("YARDI_POLL_INTERVAL_MS must be set")
        .parse::<u64>()
        .expect("YARDI_POLL_INTERVAL_MS must be a positive integer");
    Duration::from_millis(poll_interval_ms)
}

/// Upper bound for waiting on sync side effects in tests.
pub fn sync_timeout() -> Duration {
    Duration::from_secs(15)
}

/// Waits until a condition is satisfied or times out.
pub async fn wait_for_condition<F, Fut>(timeout: Duration, mut check: F) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<bool>>,
{
    let start = tokio::time::Instant::now();
    let interval = poll_interval();

    loop {
        if check().await? {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            anyhow::bail!("Timed out waiting for condition");
        }
        tokio::time::sleep(interval).await;
    }
}
