use std::sync::Arc;

use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::state::AppState;

pub mod health;
pub mod websocket;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/ws", get(websocket::handler))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_request(tower_http::trace::DefaultOnRequest::new().level(Level::INFO))
                .on_response(
                    tower_http::trace::DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(tower_http::LatencyUnit::Millis),
                ),
        )
        .with_state(state)
}
