use std::sync::Arc;

use anyhow::Result;

use crate::models::FailureNotification;
use crate::settings;
use crate::timeouts::AWS_HARD_TIMEOUT;

/// Publishes failure notifications to SNS.
pub struct FailurePublisher {
    sns_client: aws_sdk_sns::Client,
    topic_arn: String,
}

impl FailurePublisher {
    pub fn new(sns_client: aws_sdk_sns::Client) -> Arc<Self> {
        let cfg = settings::get();
        Arc::new(Self {
            sns_client,
            topic_arn: cfg.failure_sns_topic_arn.clone(),
        })
    }

    pub async fn publish(&self, notification: &FailureNotification) -> Result<()> {
        let message = serde_json::to_string(notification)?;

        tokio::time::timeout(
            AWS_HARD_TIMEOUT,
            self.sns_client
                .publish()
                .topic_arn(&self.topic_arn)
                .message(&message)
                .message_attributes(
                    "failure_type",
                    aws_sdk_sns::types::MessageAttributeValue::builder()
                        .data_type("String")
                        .string_value(format!("{:?}", notification.failure_type))
                        .build()?,
                )
                .send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("SNS publish timed out"))??;

        Ok(())
    }
}
