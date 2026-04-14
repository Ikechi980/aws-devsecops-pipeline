use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use aws_config::timeout::TimeoutConfig;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod failure_publisher;
mod models;
mod resources_client;
mod resources_event_consumer;
mod settings;
mod state;
mod sync_engine;
mod timeouts;
mod yardi_client;

use failure_publisher::FailurePublisher;
use resources_client::{ResourcesApiClient, ResourcesApiError};
use resources_event_consumer::ResourcesEventConsumer;
use state::StateManager;
use sync_engine::SyncEngine;
use timeouts::{AWS_OPERATION_TIMEOUT, FAILURE_RETRY_INTERVAL, HTTP_REQUEST_TIMEOUT};
use yardi_client::YardiClient;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    init_logging();

    settings::Settings::init_from_env()?;

    tracing::info!("Starting yardi-sync service");

    let components = init_components().await?;

    run_service(components).await?;

    tracing::info!("Shutdown complete");

    Ok(())
}

fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();
}

struct Components {
    state_manager: Arc<StateManager>,
    sync_engine: Arc<SyncEngine>,
    resources_client: Arc<ResourcesApiClient>,
    sqs_client: aws_sdk_sqs::Client,
    failure_publisher: Arc<FailurePublisher>,
}

async fn init_components() -> Result<Components> {
    let cfg = settings::get();

    let timeout_config = TimeoutConfig::builder()
        .operation_timeout(AWS_OPERATION_TIMEOUT)
        .operation_attempt_timeout(AWS_OPERATION_TIMEOUT)
        .build();

    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .timeout_config(timeout_config)
        .region(aws_config::Region::new(cfg.aws_region.clone()));

    if let Some(endpoint) = &cfg.aws_endpoint_url {
        tracing::info!(endpoint = %endpoint, "Using custom AWS endpoint");
        config_loader = config_loader.endpoint_url(endpoint);
    }

    let aws_config = config_loader.load().await;

    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);
    let sns_client = aws_sdk_sns::Client::new(&aws_config);

    let http_client = reqwest::Client::builder()
        .timeout(HTTP_REQUEST_TIMEOUT)
        .build()?;

    let state_manager = StateManager::new();
    let yardi_client = YardiClient::new(http_client.clone());
    let resources_client = Arc::new(ResourcesApiClient::new(http_client, &aws_config)?);
    let failure_publisher = FailurePublisher::new(sns_client);

    let sync_engine = Arc::new(SyncEngine::new(
        Arc::clone(&yardi_client),
        Arc::clone(&resources_client),
        Arc::clone(&state_manager),
    ));

    Ok(Components {
        state_manager,
        sync_engine,
        resources_client,
        sqs_client,
        failure_publisher,
    })
}

async fn run_service(components: Components) -> Result<()> {
    let cfg = settings::get();

    let (event_consumer_shutdown_tx, event_consumer_shutdown_rx) = oneshot::channel();
    let (poll_shutdown_tx, poll_shutdown_rx) = oneshot::channel();
    let (failure_publish_shutdown_tx, failure_publish_shutdown_rx) = oneshot::channel();

    let event_consumer =
        ResourcesEventConsumer::new(components.sqs_client, Arc::clone(&components.state_manager));

    let mut event_consumer_handle = tokio::spawn(async move {
        event_consumer.run(event_consumer_shutdown_rx).await;
    });

    let mut poll_handle = spawn_poll_loop(
        Arc::clone(&components.sync_engine),
        Arc::clone(&components.state_manager),
        Arc::clone(&components.resources_client),
        Duration::from_millis(cfg.yardi_poll_interval_ms),
        Duration::from_secs(cfg.resources_refresh_interval_secs),
        poll_shutdown_rx,
    );

    let mut failure_publish_handle = spawn_failure_publish_loop(
        Arc::clone(&components.state_manager),
        Arc::clone(&components.failure_publisher),
        FAILURE_RETRY_INTERVAL,
        failure_publish_shutdown_rx,
    );

    // Wait for either SIGTERM or any task to exit unexpectedly
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal, initiating graceful shutdown");
        }
        result = &mut event_consumer_handle => {
            if let Err(e) = result {
                tracing::error!("Event consumer task panicked: {}", e);
            } else {
                tracing::error!("Event consumer task exited unexpectedly");
            }
            tracing::error!("Initiating shutdown due to task failure");
        }
        result = &mut poll_handle => {
            if let Err(e) = result {
                tracing::error!("Poll loop task panicked: {}", e);
            } else {
                tracing::error!("Poll loop task exited unexpectedly");
            }
            tracing::error!("Initiating shutdown due to task failure");
        }
        result = &mut failure_publish_handle => {
            if let Err(e) = result {
                tracing::error!("Failure publish task panicked: {}", e);
            } else {
                tracing::error!("Failure publish task exited unexpectedly");
            }
            tracing::error!("Initiating shutdown due to task failure");
        }
    }

    let _ = event_consumer_shutdown_tx.send(());
    let _ = poll_shutdown_tx.send(());
    let _ = failure_publish_shutdown_tx.send(());

    let shutdown_timeout = Duration::from_secs(10);
    let _ = tokio::time::timeout(shutdown_timeout, async {
        // Don't await the task that already exited
        // The others will shut down gracefully
        tokio::join!(
            async {
                if !event_consumer_handle.is_finished() {
                    let _ = event_consumer_handle.await;
                }
            },
            async {
                if !poll_handle.is_finished() {
                    let _ = poll_handle.await;
                }
            },
            async {
                if !failure_publish_handle.is_finished() {
                    let _ = failure_publish_handle.await;
                }
            },
        );
    })
    .await;

    Ok(())
}

fn spawn_poll_loop(
    sync_engine: Arc<SyncEngine>,
    state_manager: Arc<StateManager>,
    resources_client: Arc<ResourcesApiClient>,
    poll_interval: Duration,
    resources_refresh_interval: Duration,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sync_interval = tokio::time::interval_at(Instant::now(), poll_interval);
        sync_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut community_refresh_interval = tokio::time::interval(resources_refresh_interval);
        community_refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;

                _ = &mut shutdown_rx => {
                    tracing::info!("Poll loop shutting down");
                    break;
                }
                _ = community_refresh_interval.tick() => {
                    tracing::debug!("Marking community list dirty on refresh interval");
                    state_manager.mark_community_list_dirty();
                }
                _ = sync_interval.tick() => {
                    if state_manager.is_community_list_dirty() {
                        tracing::info!("Refreshing tracked communities from resources-api");
                        match resources_client.list_communities().await {
                            Ok(communities) => {
                                let loaded = communities.len();
                                let stats = state_manager.sync_communities(communities);
                                state_manager.mark_community_list_refreshed();
                                tracing::info!(
                                    communities_loaded = loaded,
                                    communities_added = stats.added,
                                    communities_removed = stats.removed,
                                    tracked_communities = state_manager.tracked_community_ids().len(),
                                    "Tracked community refresh complete"
                                );
                            }
                            Err(ResourcesApiError::NotFound { reason }) => {
                                state_manager.mark_community_list_dirty();
                                tracing::error!(
                                    reason = ?reason,
                                    "Community list not found during dirty refresh"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Failed to refresh community list");
                            }
                        }
                    }

                    let community_ids = state_manager.tracked_community_ids();
                    if community_ids.is_empty() {
                        tracing::debug!("No communities to sync");
                        continue;
                    }

                    tracing::debug!(count = community_ids.len(), "Starting sync cycle");

                    for community_id in community_ids {
                        if let Err(e) = sync_engine.sync_community(community_id, resources_refresh_interval).await {
                            tracing::error!(
                                community_id = %community_id,
                                error = %e,
                                "Sync failed"
                            );
                        }
                    }
                }
            }
        }
    })
}

fn spawn_failure_publish_loop(
    state_manager: Arc<StateManager>,
    failure_publisher: Arc<FailurePublisher>,
    retry_interval: Duration,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(retry_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    tracing::info!("Failure notification loop shutting down");
                    break;
                }
                _ = interval.tick() => {
                    let ready = state_manager.ready_failure_notifications();
                    if ready.is_empty() {
                        continue;
                    }

                    for (key, notification) in ready {
                        if !state_manager.is_failure_active(&key) {
                            state_manager.clear_pending_failure(&key);
                            continue;
                        }

                        tracing::info!(
                            failure_type = ?notification.failure_type,
                            community_id = ?notification.community_id,
                            "Publishing queued failure notification"
                        );
                        match failure_publisher.publish(&notification).await {
                            Ok(_) => {
                                state_manager.clear_pending_failure(&key);
                                tracing::info!(
                                    failure_type = ?notification.failure_type,
                                    community_id = ?notification.community_id,
                                    "Published queued failure notification"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    failure_type = ?notification.failure_type,
                                    community_id = ?notification.community_id,
                                    error = %e,
                                    "Failed to publish queued failure notification, will retry"
                                );
                            }
                        }
                    }
                }
            }
        }
    })
}
