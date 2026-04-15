// v0.1.1
use crate::core_resources::CoreResourcesClient;
use crate::events_repo::MongoEventsRepo;
use crate::secrets::resolve_ssm_parameter;
use crate::settings::{Settings, get};
use crate::state::AppState;
use crate::systems::SystemsClient;
use lambda_http::{Error, run};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod core_resources;
mod error;
mod events_repo;
mod handlers;
mod identity;
mod models;
mod router;
mod secrets;
mod settings;
mod state;
mod systems;

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

    Settings::init_from_env().map_err(lambda_http::Error::from)?;
    let cfg = get();

    if cfg.allow_unauthenticated {
        tracing::warn!(
            "ALLOW_UNAUTHENTICATED is enabled; this should only be used for local development"
        );
    }

    let aws_config = if let Ok(endpoint) = std::env::var("AWS_ENDPOINT_URL") {
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .load()
            .await
    } else {
        aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await
    };
    let ssm_client = aws_sdk_ssm::Client::new(&aws_config);
    let events_mongo_url = resolve_ssm_parameter(&ssm_client, &cfg.events_mongo_url_ssm_parameter)
        .await
        .map_err(lambda_http::Error::from)?;

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(lambda_http::Error::from)?;

    let systems = SystemsClient::new(http_client.clone(), cfg.systems_api_base_url.clone());
    let core_resources = CoreResourcesClient::new(
        http_client,
        cfg.core_resources_api_base_url.clone(),
        &aws_config,
    )
    .map_err(lambda_http::Error::from)?;
    let events_repo = MongoEventsRepo::new(events_mongo_url);

    let state = AppState {
        systems,
        core_resources,
        events_repo: Arc::new(events_repo),
        events_limit_default: cfg.events_limit_default,
        events_limit_max: cfg.events_limit_max,
        allow_unauthenticated: cfg.allow_unauthenticated,
    };

    let app = router::create_router(state);

    run(app).await
}
