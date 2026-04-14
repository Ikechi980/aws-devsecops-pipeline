use std::sync::Arc;

use anyhow::Result;

use crate::models::{ChangeEvent, ChangeEventEnvelope, ChangeEventType, Community};
use crate::settings;
use crate::state::StateManager;
use crate::timeouts::AWS_HARD_TIMEOUT;

/// Consumes resources-api change events from an SQS queue.
pub struct ResourcesEventConsumer {
    sqs_client: aws_sdk_sqs::Client,
    queue_url: String,
    state_manager: Arc<StateManager>,
}

impl ResourcesEventConsumer {
    pub fn new(sqs_client: aws_sdk_sqs::Client, state_manager: Arc<StateManager>) -> Self {
        let cfg = settings::get();
        Self {
            sqs_client,
            queue_url: cfg.resources_events_queue_url.clone(),
            state_manager,
        }
    }

    pub async fn run(&self, mut shutdown: tokio::sync::oneshot::Receiver<()>) {
        tracing::info!(queue_url = %self.queue_url, "Starting resources-api event consumer");

        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    tracing::info!("Resources-api event consumer shutting down");
                    break;
                }
                result = self.poll_messages() => {
                    if let Err(e) = result {
                        tracing::warn!("Failed to poll SQS: {:#}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    async fn poll_messages(&self) -> Result<()> {
        tracing::debug!("Polling SQS for messages");

        let response = tokio::time::timeout(
            AWS_HARD_TIMEOUT,
            self.sqs_client
                .receive_message()
                .queue_url(&self.queue_url)
                .max_number_of_messages(10)
                .wait_time_seconds(20)
                .send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("SQS receive_message timed out"))??;

        let messages = response.messages();
        if messages.is_empty() {
            return Ok(());
        }

        tracing::info!(
            message_count = messages.len(),
            "Processing resources-api event batch"
        );

        let mut processed_messages = 0usize;
        let mut failed_messages = 0usize;

        for message in messages {
            let receipt_handle = message.receipt_handle().unwrap_or_default();
            tracing::debug!(
                receipt_handle = receipt_handle,
                message_id = message.message_id().unwrap_or_default(),
                "Processing SQS message"
            );

            if let Some(body) = message.body() {
                match self.process_message(body).await {
                    Ok(_) => {
                        processed_messages += 1;
                    }
                    Err(e) => {
                        failed_messages += 1;
                        tracing::warn!("Failed to process SQS message: {:#}", e);
                    }
                }
            } else {
                failed_messages += 1;
                tracing::warn!(
                    message_id = message.message_id().unwrap_or_default(),
                    "SQS message missing body"
                );
            }

            // Delete the message regardless of processing success to avoid reprocessing
            tokio::time::timeout(
                AWS_HARD_TIMEOUT,
                self.sqs_client
                    .delete_message()
                    .queue_url(&self.queue_url)
                    .receipt_handle(receipt_handle)
                    .send(),
            )
            .await
            .map_err(|_| anyhow::anyhow!("SQS delete_message timed out"))??;
        }

        if failed_messages == 0 {
            tracing::info!(
                message_count = processed_messages,
                "Resources-api event batch processed successfully"
            );
        } else {
            tracing::warn!(
                processed_messages,
                failed_messages,
                "Resources-api event batch completed with message failures"
            );
        }

        Ok(())
    }

    async fn process_message(&self, body: &str) -> Result<()> {
        // SNS wraps the message in an envelope
        let envelope: ChangeEventEnvelope = serde_json::from_str(body)?;
        let event: ChangeEvent = serde_json::from_str(&envelope.message)?;

        tracing::debug!(
            resource_type = %event.resource_type,
            "Received change event"
        );

        match event.resource_type.as_str() {
            "community" => self.handle_community_event(&event).await?,
            "location" => self.handle_location_event(&event).await,
            "resident" => self.handle_resident_event(&event).await,
            _ => {
                tracing::debug!(
                    resource_type = %event.resource_type,
                    "Ignoring unknown resource type"
                );
            }
        }

        Ok(())
    }

    async fn handle_community_event(&self, event: &ChangeEvent) -> Result<()> {
        match &event.event {
            ChangeEventType::Create { after } | ChangeEventType::Update { after, .. } => {
                let community: Community = serde_json::from_value(after.clone())?;
                let was_tracked = self.state_manager.get_community(community.id).is_some();

                if community.has_yardi_integration() {
                    self.state_manager.upsert_community(community.clone());
                    if was_tracked {
                        tracing::info!(
                            community_id = %community.id,
                            community_name = %community.name,
                            "Updated tracked community after community event"
                        );
                    } else {
                        tracing::info!(
                            community_id = %community.id,
                            community_name = %community.name,
                            "Started tracking community after community event"
                        );
                    }
                } else {
                    self.state_manager.remove_community(community.id);
                    if was_tracked {
                        tracing::info!(
                            community_id = %community.id,
                            community_name = %community.name,
                            "Stopped tracking community after community event"
                        );
                    } else {
                        tracing::debug!(
                            community_id = %community.id,
                            community_name = %community.name,
                            "Ignoring community event for untracked community without Yardi integration"
                        );
                    }
                }
            }
            ChangeEventType::Delete { before } => {
                let community: Community = serde_json::from_value(before.clone())?;
                let was_tracked = self.state_manager.get_community(community.id).is_some();
                self.state_manager.remove_community(community.id);
                if was_tracked {
                    tracing::info!(
                        community_id = %community.id,
                        "Stopped tracking deleted community"
                    );
                } else {
                    tracing::debug!(
                        community_id = %community.id,
                        "Ignoring delete event for untracked community"
                    );
                }
            }
        }

        Ok(())
    }

    async fn handle_location_event(&self, event: &ChangeEvent) {
        let community_id = extract_community_id(&event.event);

        if let Some(id) = community_id
            && self.state_manager.get_community(id).is_some()
        {
            self.state_manager.mark_dirty(id);
            tracing::info!(
                community_id = %id,
                "Marked tracked community dirty from location event"
            );
        }
    }

    async fn handle_resident_event(&self, event: &ChangeEvent) {
        let community_id = extract_community_id(&event.event);

        if let Some(id) = community_id
            && self.state_manager.get_community(id).is_some()
        {
            self.state_manager.mark_dirty(id);
            tracing::info!(
                community_id = %id,
                "Marked tracked community dirty from resident event"
            );
        }
    }
}

fn extract_community_id(event: &ChangeEventType) -> Option<uuid::Uuid> {
    let value = match event {
        ChangeEventType::Create { after } | ChangeEventType::Update { after, .. } => {
            after.get("community_id")
        }
        ChangeEventType::Delete { before } => before.get("community_id"),
    };

    value.and_then(|v| v.as_str().and_then(|s| s.parse().ok()))
}
