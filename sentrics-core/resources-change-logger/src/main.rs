use anyhow::Result;
use aws_lambda_events::event::sqs::SqsEvent;
use lambda_runtime::{LambdaEvent, service_fn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod events;
mod processor;
mod settings;
mod storage;

use processor::ChangeLogger;
use settings::{Settings, get};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    tracing::info!("Initializing resources-change-logger Lambda");

    Settings::init_from_env()?;
    let cfg = get();

    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

    if let Some(endpoint) = &cfg.aws_endpoint_url {
        tracing::info!(endpoint = %endpoint, "Using custom AWS endpoint");
        config_loader = config_loader.endpoint_url(endpoint);
    }

    let aws_config = config_loader.load().await;
    let dynamodb = aws_sdk_dynamodb::Client::new(&aws_config);

    let logger = ChangeLogger::new(dynamodb, cfg.change_log_table_name.clone());

    let handler = service_fn(|event: LambdaEvent<SqsEvent>| {
        let logger = logger.clone();
        async move { Ok::<_, lambda_runtime::Error>(logger.handle_sqs_event(event.payload).await) }
    });

    lambda_runtime::run(handler)
        .await
        .map_err(|err| anyhow::anyhow!(err))?;

    Ok(())
}
