use axum::{
    Router,
    routing::{get, post},
};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::state::AppState;

pub mod certificates;
pub mod health;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/certificates", post(certificates::handler))
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
        .layer(RequestBodyLimitLayer::new(256 * 1024)) // 256KB limit (CSRs are small)
        .with_state(state)
}
