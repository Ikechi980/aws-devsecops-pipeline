use std::env;
use std::time::Duration;

use anyhow::Result;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_sns::Client as SnsClient;
use aws_sdk_sqs::Client as SqsClient;
use aws_types::region::Region;

pub struct AwsTestClients {
    pub sns: SnsClient,
    pub sqs: SqsClient,
}

pub async fn clients() -> AwsTestClients {
    let region = RegionProviderChain::first_try(env::var("AWS_REGION").ok().map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-east-1"));

    let mut loader = aws_config::from_env().region(region);
    if let Ok(endpoint) = env::var("AWS_ENDPOINT_URL") {
        loader = loader.endpoint_url(endpoint);
    }
    let config = loader.load().await;

    AwsTestClients {
        sns: SnsClient::new(&config),
        sqs: SqsClient::new(&config),
    }
}

pub async fn create_queue(sqs: &SqsClient, name: &str) -> Result<String> {
    let response = sqs.create_queue().queue_name(name).send().await?;
    Ok(response.queue_url().unwrap_or_default().to_string())
}

pub async fn delete_queue(sqs: &SqsClient, queue_url: &str) -> Result<()> {
    sqs.delete_queue().queue_url(queue_url).send().await?;
    Ok(())
}

pub async fn get_queue_arn(sqs: &SqsClient, queue_url: &str) -> Result<String> {
    let response = sqs
        .get_queue_attributes()
        .queue_url(queue_url)
        .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await?;
    let arn = response
        .attributes()
        .and_then(|attrs| attrs.get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn))
        .cloned()
        .unwrap_or_default();
    Ok(arn)
}

pub async fn subscribe_queue_to_topic(
    sns: &SnsClient,
    sqs: &SqsClient,
    topic_arn: &str,
    queue_url: &str,
) -> Result<String> {
    let queue_arn = get_queue_arn(sqs, queue_url).await?;
    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "AllowSNSPublish",
            "Effect": "Allow",
            "Principal": { "AWS": "*" },
            "Action": "sqs:SendMessage",
            "Resource": queue_arn,
            "Condition": { "ArnEquals": { "aws:SourceArn": topic_arn } }
        }]
    });

    sqs.set_queue_attributes()
        .queue_url(queue_url)
        .attributes(
            aws_sdk_sqs::types::QueueAttributeName::Policy,
            policy.to_string(),
        )
        .send()
        .await?;

    let response = sns
        .subscribe()
        .topic_arn(topic_arn)
        .protocol("sqs")
        .endpoint(queue_arn)
        .return_subscription_arn(true)
        .send()
        .await?;

    Ok(response.subscription_arn().unwrap_or_default().to_string())
}

pub async fn unsubscribe(sns: &SnsClient, subscription_arn: &str) -> Result<()> {
    if !subscription_arn.is_empty() {
        sns.unsubscribe()
            .subscription_arn(subscription_arn)
            .send()
            .await?;
    }
    Ok(())
}

pub async fn send_raw_message(sqs: &SqsClient, queue_url: &str, body: &str) -> Result<()> {
    sqs.send_message()
        .queue_url(queue_url)
        .message_body(body)
        .send()
        .await?;
    Ok(())
}

pub async fn expect_no_sns_message(
    sqs: &SqsClient,
    queue_url: &str,
    timeout: Duration,
) -> Result<()> {
    let start = tokio::time::Instant::now();

    loop {
        let response = sqs
            .receive_message()
            .queue_url(queue_url)
            .max_number_of_messages(1)
            .wait_time_seconds(1)
            .send()
            .await?;

        if let Some(message) = response.messages().first() {
            if let Some(receipt) = message.receipt_handle() {
                sqs.delete_message()
                    .queue_url(queue_url)
                    .receipt_handle(receipt)
                    .send()
                    .await?;
            }

            anyhow::bail!(
                "Unexpected SNS message received: {}",
                message.body().unwrap_or_default()
            );
        }

        if start.elapsed() >= timeout {
            return Ok(());
        }
    }
}
