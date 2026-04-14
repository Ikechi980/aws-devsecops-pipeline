use std::{sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use axum::Router;
use tokio::{net::TcpListener, sync::watch, task::JoinHandle};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod client;
mod routes;
mod settings;
mod sqs_worker;
mod state;

use sqs_worker::SqsWorker;
use state::AppState;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .init();

    settings::Settings::init_from_env()?;
    let cfg = settings::get();

    let aws_config = if let Some(endpoint) = &cfg.aws_endpoint_url {
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(endpoint)
            .load()
            .await
    } else {
        aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await
    };

    let sqs_client = aws_sdk_sqs::Client::new(&aws_config);
    let sns_client = aws_sdk_sns::Client::new(&aws_config);
    let app_state = Arc::new(AppState::new());

    let mut sqs_worker = SqsWorker::new(
        sqs_client,
        sns_client,
        cfg.sns_topic_arn.clone(),
        Arc::clone(&app_state),
    );

    let queue_url = sqs_worker.setup().await?;
    tracing::info!("Created ephemeral SQS queue: {}", queue_url);

    let listener = TcpListener::bind(cfg.addr).await?;
    tracing::info!("serving on http://{}", cfg.addr);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let worker_handle = spawn_sqs_worker(sqs_worker, shutdown_rx.clone());
    let server_handle = spawn_http_server(
        listener,
        routes::create_router(app_state.clone()),
        shutdown_rx,
    );

    let signal = wait_for_shutdown_signal().await;
    initiate_shutdown(&app_state, &shutdown_tx, signal);
    wait_for_shutdown_tasks(server_handle, worker_handle).await?;

    tracing::info!("Shutdown complete");
    Ok(())
}

fn spawn_sqs_worker(worker: SqsWorker, shutdown: watch::Receiver<bool>) -> JoinHandle<()> {
    tokio::spawn(async move {
        worker.run(shutdown).await;
    })
}

fn spawn_http_server(
    listener: TcpListener,
    app: Router,
    shutdown: watch::Receiver<bool>,
) -> JoinHandle<std::io::Result<()>> {
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(wait_for_shutdown_flag(shutdown))
            .await
    })
}

async fn wait_for_shutdown_flag(mut shutdown: watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }

    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            break;
        }
    }
}

fn initiate_shutdown(app_state: &AppState, shutdown_tx: &watch::Sender<bool>, signal: &str) {
    tracing::info!(
        signal,
        "Received shutdown signal, closing connections and cleaning up"
    );
    app_state.begin_shutdown();
    let _ = shutdown_tx.send(true);
}

async fn wait_for_shutdown_tasks(
    server_handle: JoinHandle<std::io::Result<()>>,
    worker_handle: JoinHandle<()>,
) -> Result<()> {
    match tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
        let server_result = server_handle.await;
        let worker_result = worker_handle.await;
        (server_result, worker_result)
    })
    .await
    {
        Ok((server_result, worker_result)) => {
            match server_result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => return Err(err.into()),
                Err(err) => return Err(anyhow!("HTTP server task failed: {err}")),
            }

            if let Err(err) = worker_result {
                return Err(anyhow!("SQS worker task failed: {err}"));
            }
        }
        Err(_) => {
            tracing::warn!(
                timeout_secs = SHUTDOWN_TIMEOUT.as_secs(),
                "Shutdown timed out; exiting before all tasks completed"
            );
        }
    }

    Ok(())
}

async fn wait_for_shutdown_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate =
            signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");

        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.expect("Failed to install CTRL+C handler");
                "SIGINT"
            }
            _ = terminate.recv() => "SIGTERM",
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
        "SIGINT"
    }
}
