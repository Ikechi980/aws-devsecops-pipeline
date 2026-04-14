use anyhow::{Result, anyhow};
use aws_lambda_events::event::sqs::{SqsBatchResponse, SqsEvent, SqsMessage};
use serde_json::Value;

use crate::events::{ChangeEventRouting, SnsEnvelope};
use crate::publisher::HeadendPublisher;
use crate::systems::SystemsClient;

#[derive(Clone)]
pub struct ChangeProcessor {
    systems: SystemsClient,
    publisher: HeadendPublisher,
}

impl ChangeProcessor {
    pub fn new(systems: SystemsClient, publisher: HeadendPublisher) -> Self {
        Self { systems, publisher }
    }

    pub async fn handle_sqs_event(&self, event: SqsEvent) -> SqsBatchResponse {
        let mut response = SqsBatchResponse::default();

        for record in event.records {
            match self.process_message(&record).await {
                Ok(()) => {}
                Err(ProcessError::Retryable(err)) => {
                    let message_id = record
                        .message_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    tracing::error!(%message_id, error = ?err, "Failed to process SQS message");
                    response.add_failure(message_id);
                }
                Err(ProcessError::Permanent(err)) => {
                    let message_id = record
                        .message_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    tracing::warn!(%message_id, error = ?err, "Dropping non-retryable SQS message");
                }
            }
        }

        response
    }

    pub async fn process_body(
        &self,
        message_id: Option<&str>,
        body: &str,
    ) -> Result<(), ProcessError> {
        let raw_message = extract_sns_message(body).map_err(ProcessError::permanent)?;
        let raw_change_event: Value =
            serde_json::from_str(&raw_message).map_err(ProcessError::permanent)?;
        let change_event: ChangeEventRouting =
            serde_json::from_value(raw_change_event.clone()).map_err(ProcessError::permanent)?;

        let core_community_id = match change_event.core_community_id() {
            Some(id) => id,
            None => {
                return Err(ProcessError::permanent(anyhow!(
                    "missing community identifier"
                )));
            }
        };

        let system = self
            .systems
            .find_by_core_community_id(&core_community_id)
            .await
            .map_err(ProcessError::retryable)?;

        let Some(system) = system else {
            tracing::info!(
                %core_community_id,
                resource_type = %change_event.resource_type,
                event_type = %change_event.event_type(),
                "No mapped Ensure community found, skipping"
            );
            return Ok(());
        };

        let target_community_id = system.community_id.to_ascii_lowercase();

        self.publisher
            .publish(&target_community_id, &raw_change_event)
            .await
            .map_err(ProcessError::retryable)?;

        tracing::info!(
            message_id = message_id.unwrap_or("unknown"),
            %core_community_id,
            ensure_community_id = %target_community_id,
            resource_type = %change_event.resource_type,
            event_type = %change_event.event_type(),
            "Published change event to headend topic"
        );

        Ok(())
    }

    async fn process_message(&self, record: &SqsMessage) -> Result<(), ProcessError> {
        let body = record
            .body
            .as_deref()
            .ok_or_else(|| ProcessError::permanent(anyhow!("SQS message missing body")))?;

        self.process_body(record.message_id.as_deref(), body).await
    }
}

fn extract_sns_message(body: &str) -> Result<String> {
    let envelope = serde_json::from_str::<SnsEnvelope>(body)
        .map_err(|err| anyhow!("Expected SNS envelope: {err}"))?;
    Ok(envelope.message)
}

#[derive(Debug)]
pub enum ProcessError {
    Retryable(anyhow::Error),
    Permanent(anyhow::Error),
}

impl ProcessError {
    pub fn retryable<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Retryable(err.into())
    }

    pub fn permanent<E: Into<anyhow::Error>>(err: E) -> Self {
        Self::Permanent(err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publisher::HeadendPublisher;
    use crate::systems::SystemsClient;
    use aws_config::BehaviorVersion;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_systems_server(status: &str, body: &str) -> anyhow::Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let status = status.to_string();
        let body = body.to_string();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer).await;
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        Ok(format!("http://{}", addr))
    }

    async fn build_processor(
        systems_base_url: String,
        sns_endpoint: &str,
    ) -> anyhow::Result<ChangeProcessor> {
        let http_client = reqwest::Client::new();
        let systems = SystemsClient::new(http_client, systems_base_url);

        let aws_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(sns_endpoint)
            .region(aws_config::Region::new("us-east-1"))
            .credentials_provider(aws_sdk_sns::config::Credentials::new(
                "test", "test", None, None, "test",
            ))
            .load()
            .await;

        let sns_client = aws_sdk_sns::Client::new(&aws_config);
        let publisher = HeadendPublisher::new(
            sns_client,
            "arn:aws:sns:us-east-1:000000000000:headend-messages".to_string(),
        );

        Ok(ChangeProcessor::new(systems, publisher))
    }

    fn build_change_event(core_community_id: &str) -> String {
        serde_json::json!({
            "event_id": "00000000-0000-0000-0000-000000000000",
            "resource_type": "community",
            "timestamp": "2024-01-15T10:30:00Z",
            "event_type": "update",
            "requester": { "type": "service", "role_name": "local-dev" },
            "after": { "id": core_community_id, "name": "Alpha" },
            "before": { "id": core_community_id, "name": "Alpha" }
        })
        .to_string()
    }

    fn wrap_sns_message(message: &str) -> String {
        serde_json::json!({
            "Type": "Notification",
            "MessageId": "test-message",
            "TopicArn": "arn:aws:sns:us-east-1:000000000000:core-change-events",
            "Message": message
        })
        .to_string()
    }

    #[tokio::test]
    async fn systems_api_500_is_retryable() -> anyhow::Result<()> {
        let base_url = spawn_mock_systems_server("500 Internal Server Error", "{}").await?;
        let processor = build_processor(base_url, "http://127.0.0.1:1").await?;

        let event_body = build_change_event("11111111-1111-1111-1111-111111111111");
        let sns_body = wrap_sns_message(&event_body);

        let err = processor
            .process_body(Some("test-500"), &sns_body)
            .await
            .expect_err("expected retryable error");

        assert!(matches!(err, ProcessError::Retryable(_)));
        Ok(())
    }

    #[tokio::test]
    async fn systems_api_bad_json_is_retryable() -> anyhow::Result<()> {
        let base_url = spawn_mock_systems_server("200 OK", "not-json").await?;
        let processor = build_processor(base_url, "http://127.0.0.1:1").await?;

        let event_body = build_change_event("11111111-1111-1111-1111-111111111111");
        let sns_body = wrap_sns_message(&event_body);

        let err = processor
            .process_body(Some("test-bad-json"), &sns_body)
            .await
            .expect_err("expected retryable error");

        assert!(matches!(err, ProcessError::Retryable(_)));
        Ok(())
    }

    #[tokio::test]
    async fn publish_failure_is_retryable() -> anyhow::Result<()> {
        let systems_body = json!([
            {
                "communityId": "alpha",
                "coreCommunityId": "11111111-1111-1111-1111-111111111111"
            }
        ])
        .to_string();

        let base_url = spawn_mock_systems_server("200 OK", &systems_body).await?;
        let processor = build_processor(base_url, "http://127.0.0.1:1").await?;

        let event_body = build_change_event("11111111-1111-1111-1111-111111111111");
        let sns_body = wrap_sns_message(&event_body);

        let err = processor
            .process_body(Some("test-publish"), &sns_body)
            .await
            .expect_err("expected retryable error");

        assert!(matches!(err, ProcessError::Retryable(_)));
        Ok(())
    }

    #[tokio::test]
    async fn non_sns_body_is_permanent() -> anyhow::Result<()> {
        let base_url = spawn_mock_systems_server("200 OK", "[]").await?;
        let processor = build_processor(base_url, "http://127.0.0.1:1").await?;

        let event_body = build_change_event("11111111-1111-1111-1111-111111111111");
        let err = processor
            .process_body(Some("test-non-sns"), &event_body)
            .await
            .expect_err("expected permanent error");

        assert!(matches!(err, ProcessError::Permanent(_)));
        Ok(())
    }
}
