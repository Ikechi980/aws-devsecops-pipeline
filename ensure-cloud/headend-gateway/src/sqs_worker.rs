use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use uuid::Uuid;

use crate::state::AppState;

pub struct SqsWorker {
    sqs_client: aws_sdk_sqs::Client,
    sns_client: aws_sdk_sns::Client,
    sns_topic_arn: String,
    app_state: Arc<AppState>,
    queue_url: Option<String>,
    subscription_arn: Option<String>,
}

impl SqsWorker {
    pub fn new(
        sqs_client: aws_sdk_sqs::Client,
        sns_client: aws_sdk_sns::Client,
        sns_topic_arn: String,
        app_state: Arc<AppState>,
    ) -> Self {
        Self {
            sqs_client,
            sns_client,
            sns_topic_arn,
            app_state,
            queue_url: None,
            subscription_arn: None,
        }
    }

    /// Creates an ephemeral SQS queue and subscribes it to the SNS topic.
    ///
    /// The queue has a short message retention period (60 seconds). On graceful shutdown,
    /// the worker unsubscribes from SNS and deletes the queue. If the process crashes, the
    /// queue remains until AWS garbage collects it after all messages expire. Empty queues
    /// may persist but have minimal cost.
    pub async fn setup(&mut self) -> anyhow::Result<String> {
        let queue_name = format!("headend-gateway-{}", Uuid::new_v4());

        let create_result = self
            .sqs_client
            .create_queue()
            .queue_name(&queue_name)
            .attributes(
                aws_sdk_sqs::types::QueueAttributeName::MessageRetentionPeriod,
                "60",
            )
            .send()
            .await?;

        let queue_url = create_result
            .queue_url()
            .ok_or_else(|| anyhow::anyhow!("No queue URL returned"))?
            .to_string();

        let attrs = self
            .sqs_client
            .get_queue_attributes()
            .queue_url(&queue_url)
            .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
            .send()
            .await?;

        let queue_arn = attrs
            .attributes()
            .and_then(|a| a.get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn))
            .ok_or_else(|| anyhow::anyhow!("Could not get queue ARN"))?;

        let policy = serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Allow",
                "Principal": {"Service": "sns.amazonaws.com"},
                "Action": "sqs:SendMessage",
                "Resource": queue_arn,
                "Condition": {
                    "ArnEquals": {
                        "aws:SourceArn": &self.sns_topic_arn
                    }
                }
            }]
        });

        self.sqs_client
            .set_queue_attributes()
            .queue_url(&queue_url)
            .attributes(
                aws_sdk_sqs::types::QueueAttributeName::Policy,
                policy.to_string(),
            )
            .send()
            .await?;

        let sub_result = self
            .sns_client
            .subscribe()
            .topic_arn(&self.sns_topic_arn)
            .protocol("sqs")
            .endpoint(queue_arn)
            .attributes("RawMessageDelivery", "true")
            .send()
            .await?;

        self.subscription_arn = sub_result.subscription_arn().map(|s| s.to_string());
        self.queue_url = Some(queue_url.clone());

        Ok(queue_url)
    }

    /// Continuously polls the SQS queue and routes messages to connected clients.
    ///
    /// Runs until the shutdown signal is received, then performs cleanup.
    pub async fn run(mut self, mut shutdown: watch::Receiver<bool>) {
        let queue_url = match &self.queue_url {
            Some(url) => url.clone(),
            None => {
                tracing::error!("SQS worker started without queue URL");
                return;
            }
        };

        tracing::info!("Starting SQS worker polling loop");

        loop {
            if *shutdown.borrow() {
                tracing::info!("SQS worker received shutdown signal");
                break;
            }

            tokio::select! {
                result = shutdown.changed() => {
                    match result {
                        Ok(()) => {
                            if *shutdown.borrow() {
                                tracing::info!("SQS worker received shutdown signal");
                                break;
                            }
                        }
                        Err(_) => {
                            tracing::info!("SQS worker shutdown channel closed");
                            break;
                        }
                    }
                }
                result = self.sqs_client
                    .receive_message()
                    .queue_url(&queue_url)
                    .wait_time_seconds(20)
                    .max_number_of_messages(10)
                    .send() => {
                    match result {
                        Ok(response) => {
                            for msg in response.messages.unwrap_or_default() {
                                if let Some(body) = msg.body() {
                                    self.process_message(body);
                                }

                                if let Some(receipt) = msg.receipt_handle() {
                                    let _ = self
                                        .sqs_client
                                        .delete_message()
                                        .queue_url(&queue_url)
                                        .receipt_handle(receipt)
                                        .send()
                                        .await;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("SQS receive error: {:?}", e);
                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }

        self.cleanup().await;
    }

    async fn cleanup(&mut self) {
        tracing::info!("Cleaning up SQS resources");

        if let Some(sub_arn) = &self.subscription_arn {
            if let Err(e) = self
                .sns_client
                .unsubscribe()
                .subscription_arn(sub_arn)
                .send()
                .await
            {
                tracing::warn!("Failed to unsubscribe from SNS: {:?}", e);
            } else {
                tracing::info!("Unsubscribed from SNS topic");
            }
        }

        if let Some(queue_url) = &self.queue_url {
            if let Err(e) = self
                .sqs_client
                .delete_queue()
                .queue_url(queue_url)
                .send()
                .await
            {
                tracing::warn!("Failed to delete SQS queue: {:?}", e);
            } else {
                tracing::info!("Deleted SQS queue");
            }
        }
    }

    fn process_message(&self, json_body: &str) {
        match serde_json::from_str::<Payload>(json_body) {
            Ok(payload) => {
                if !is_valid_community_id(&payload.target_community_id) {
                    tracing::warn!(
                        target_community_id = %payload.target_community_id,
                        "Invalid message payload: target_community_id must match [a-z0-9-]+"
                    );
                    return;
                }

                if payload.message_type.is_empty() {
                    tracing::warn!("Incoming message missing message_type");
                    return;
                }

                let forward_payload = match build_forward_payload(&payload) {
                    Ok(payload) => payload,
                    Err(err) => {
                        tracing::warn!("Invalid message payload: {}", err);
                        return;
                    }
                };

                if self
                    .app_state
                    .send_to_client(&payload.target_community_id, forward_payload)
                {
                    tracing::debug!(
                        "Message routed to local client {}",
                        payload.target_community_id
                    );
                } else {
                    tracing::debug!(
                        "Target client not connected on this instance: {}",
                        payload.target_community_id
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Failed to parse SQS message: {}", e);
            }
        }
    }
}

fn build_forward_payload(payload: &Payload) -> Result<String, &'static str> {
    if payload.versions.is_empty() {
        return Err("versions must include at least one entry");
    }

    let mut versions = std::collections::HashSet::new();
    for item in &payload.versions {
        if !versions.insert(item.version) {
            return Err("versions must have unique version numbers");
        }
    }

    let forward = serde_json::json!({
        "message_type": payload.message_type,
        "versions": payload.versions,
    });

    serde_json::to_string(&forward).map_err(|_| "failed to serialize payload")
}

fn is_valid_community_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    target_community_id: String,
    message_type: String,
    versions: Vec<VersionedPayload>,
}

#[derive(Deserialize, Serialize)]
struct VersionedPayload {
    version: u32,
    payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::{Payload, VersionedPayload, build_forward_payload, is_valid_community_id};

    #[test]
    fn payload_accepts_expected_schema() {
        let json = r#"{
            "target_community_id": "dk-6",
            "message_type": "core_change_event",
            "versions": [{"version": 1, "payload": {"ok": true}}]
        }"#;

        let payload: Payload = serde_json::from_str(json).expect("payload should parse");
        assert_eq!(payload.target_community_id, "dk-6");
        assert_eq!(payload.message_type, "core_change_event");
        assert_eq!(payload.versions.len(), 1);
    }

    #[test]
    fn payload_rejects_legacy_target_cn() {
        let json = r#"{
            "target_cn": "dk-6.ensurelink.net",
            "message_type": "core_change_event",
            "versions": [{"version": 1, "payload": {"ok": true}}]
        }"#;

        assert!(serde_json::from_str::<Payload>(json).is_err());
    }

    #[test]
    fn payload_rejects_unknown_fields() {
        let json = r#"{
            "target_community_id": "dk-6",
            "message_type": "core_change_event",
            "versions": [{"version": 1, "payload": {"ok": true}}],
            "target_cn": "dk-6.ensurelink.net"
        }"#;

        assert!(serde_json::from_str::<Payload>(json).is_err());
    }

    #[test]
    fn valid_community_ids_accept_lowercase_slug() {
        assert!(is_valid_community_id("alpha-123"));
        assert!(is_valid_community_id("dk-6"));
    }

    #[test]
    fn invalid_community_ids_reject_uppercase_and_symbols() {
        assert!(!is_valid_community_id("Alpha"));
        assert!(!is_valid_community_id("alpha_123"));
        assert!(!is_valid_community_id(""));
    }

    #[test]
    fn forward_payload_requires_unique_versions() {
        let payload = Payload {
            target_community_id: "alpha".to_string(),
            message_type: "test".to_string(),
            versions: vec![
                VersionedPayload {
                    version: 1,
                    payload: serde_json::json!("one"),
                },
                VersionedPayload {
                    version: 1,
                    payload: serde_json::json!("two"),
                },
            ],
        };

        assert_eq!(
            build_forward_payload(&payload),
            Err("versions must have unique version numbers")
        );
    }
}
