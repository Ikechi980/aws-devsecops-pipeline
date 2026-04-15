// v0.1.1
use lambda_http::{Error, run};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::{env, time::Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod error;
mod events;
mod handlers;
mod models;
mod requester;
mod router;
mod state;

use events::EventPublisher;
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    tracing::info!("Initializing Lambda function");

    if std::env::var("ALLOW_UNAUTHENTICATED").is_ok() {
        tracing::warn!("ALLOW_UNAUTHENTICATED is enabled - local development mode");
    } else {
        tracing::info!("Production authentication mode - Cognito/IAM required");
    }

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let sns_topic_arn = env::var("SNS_TOPIC_ARN").expect("SNS_TOPIC_ARN must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect(&database_url)
        .await
        .expect("Failed to create database pool");

    tracing::info!("Database pool created successfully");

    let aws_config = aws_config::load_from_env().await;
    let sns_client = aws_sdk_sns::Client::new(&aws_config);

    let publisher = EventPublisher::new(sns_client, sns_topic_arn);

    let state = AppState {
        pool: pool.clone(),
        publisher: Arc::new(publisher),
    };

    let app = router::create_router(state);

    run(app).await?;

    pool.close().await;
    Ok(())
}
