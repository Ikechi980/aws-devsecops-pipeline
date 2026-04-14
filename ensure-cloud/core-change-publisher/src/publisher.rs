use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const SNS_PUBLISH_TIMEOUT_SECS: u64 = 5;

#[derive(Clone)]
pub struct HeadendPublisher {
    sns_client: aws_sdk_sns::Client,
    topic_arn: String,
}

impl HeadendPublisher {
    pub fn new(sns_client: aws_sdk_sns::Client, topic_arn: String) -> Self {
        Self {
            sns_client,
            topic_arn,
        }
    }

    pub async fn publish(&self, target_community_id: &str, change_event: &Value) -> Result<()> {
        #[derive(Serialize)]
        struct Payload<'a> {
            target_community_id: &'a str,
            message_type: &'a str,
            versions: Vec<VersionedPayload>,
        }

        #[derive(Serialize)]
        struct VersionedPayload {
            version: u32,
            payload: serde_json::Value,
        }

        let message = serde_json::to_string(&Payload {
            target_community_id,
            message_type: "core_change_event",
            versions: vec![VersionedPayload {
                version: 1,
                payload: change_event.clone(),
            }],
        })?;

        tokio::time::timeout(
            Duration::from_secs(SNS_PUBLISH_TIMEOUT_SECS),
            self.sns_client
                .publish()
                .topic_arn(&self.topic_arn)
                .message(message)
                .send(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("SNS publish timed out"))??;

        Ok(())
    }
}
