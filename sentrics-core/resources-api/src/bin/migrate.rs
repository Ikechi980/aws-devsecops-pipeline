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

async fn function_handler(_event: LambdaEvent<Request>) -> Result<Response, Error> {
    tracing::info!("Starting database migration");

    let database_url =
        env::var("DATABASE_URL").map_err(|_| "DATABASE_URL environment variable must be set")?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect(&database_url)
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

    lambda_runtime::run(service_fn(function_handler)).await
}
