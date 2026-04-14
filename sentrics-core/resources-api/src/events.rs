use crate::requester::Requester;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum ChangeEvent<T: Clone> {
    Create { after: T },
    Update { before: T, after: T },
    Delete { before: T },
}

#[derive(Debug, Serialize)]
struct EventMessage<T: Clone> {
    event_id: Uuid,
    resource_type: String,
    timestamp: String,
    requester: Requester,
    #[serde(flatten)]
    event: ChangeEvent<T>,
}

#[derive(Clone)]
pub struct EventPublisher {
    sns_client: aws_sdk_sns::Client,
    topic_arn: String,
}

impl EventPublisher {
    pub fn new(sns_client: aws_sdk_sns::Client, topic_arn: String) -> Self {
        Self {
            sns_client,
            topic_arn,
        }
    }

    pub async fn publish<T: Serialize + Clone>(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        event: ChangeEvent<T>,
        requester: Requester,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let message = EventMessage {
            event_id: Uuid::new_v4(),
            resource_type: resource_type.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            requester,
            event: event.clone(),
        };

        let message_json = serde_json::to_string(&message)?;

        self.sns_client
            .publish()
            .topic_arn(&self.topic_arn)
            .message(message_json)
            .send()
            .await?;

        tracing::info!(
            event_id = %message.event_id,
            resource_id = %resource_id,
            "Published {} event for {}",
            match &message.event {
                ChangeEvent::Create { .. } => "create",
                ChangeEvent::Update { .. } => "update",
                ChangeEvent::Delete { .. } => "delete",
            },
            resource_type
        );

        Ok(())
    }
}
