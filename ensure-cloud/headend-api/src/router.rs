use crate::handlers;
use crate::state::AppState;
use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;
use tracing::Level;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(handlers::health))
        .route("/v1/core/community", get(handlers::core_community))
        .route("/v1/core/locations", get(handlers::core_locations))
        .route("/v1/core/residents", get(handlers::core_residents))
        .route(
            "/v1/core/residents/{id}/photo",
            get(handlers::core_resident_photo),
        )
        .route("/v1/events", get(handlers::events))
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
