use lambda_runtime::{Error, LambdaEvent, service_fn};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::env;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Deserialize)]
struct Request {}

#[derive(Serialize)]
struct Response {
    message: String,
}

async fn function_handler(_event: LambdaEvent<Request>, database_url: &str) -> Result<Response, Error> {
    tracing::info!("Starting database migration");

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect(database_url)
        .await
        .map_err(|e| format!("Failed to connect to database: {}", e))?;

    tracing::info!("Connected to database, running migrations");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    tracing::info!("Migrations completed successfully");

    pool.close().await;

    Ok(Response {
        message: "Migrations completed successfully".to_string(),
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    tracing::info!("Initializing migration Lambda function");

    let aws_config = aws_config::load_from_env().await;

    let database_url = {
        let param_name = env::var("DATABASE_URL_SSM_PARAMETER")
            .map_err(|_| "DATABASE_URL_SSM_PARAMETER must be set")?;
        aws_sdk_ssm::Client::new(&aws_config)
            .get_parameter()
            .name(&param_name)
            .with_decryption(true)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch database URL from SSM: {}", e))?
            .parameter
            .ok_or("SSM parameter not found")?
            .value
            .ok_or("SSM parameter has no value")?
    };

    lambda_runtime::run(service_fn(|event| function_handler(event, &database_url))).await
}
