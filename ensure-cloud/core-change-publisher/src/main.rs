// v0.1.1
use anyhow::Result;
use aws_lambda_events::event::sqs::SqsEvent;
use lambda_runtime::{LambdaEvent, service_fn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod events;
mod processor;
mod publisher;
mod settings;
mod systems;

use processor::ChangeProcessor;
use publisher::HeadendPublisher;
use settings::{Settings, get};
use systems::SystemsClient;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    tracing::info!("Initializing Lambda function");

    Settings::init_from_env()?;
    let cfg = get();

    let aws_config = if let Some(endpoint) = &cfg.aws_endpoint_url {
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .load()
            .await
    } else {
        aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await
    };

    let sns_client = aws_sdk_sns::Client::new(&aws_config);
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let systems = SystemsClient::new(http_client, cfg.systems_api_base_url.clone());
    let publisher = HeadendPublisher::new(sns_client, cfg.headend_sns_topic_arn.clone());
    let processor = ChangeProcessor::new(systems, publisher);

    let handler = service_fn(|event: LambdaEvent<SqsEvent>| {
        let processor = processor.clone();
        async move { Ok::<_, lambda_runtime::Error>(processor.handle_sqs_event(event.payload).await) }
    });

    lambda_runtime::run(handler)
        .await
        .map_err(|err| anyhow::anyhow!(err))?;

    Ok(())
}
