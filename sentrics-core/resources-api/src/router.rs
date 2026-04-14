use crate::handlers;
use crate::state::AppState;
use axum::{Json, Router, extract::State, http::Method, response::IntoResponse, routing::get};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::Level;

pub fn create_router(state: AppState) -> Router {
    // CORS policy for standard API resources (allows all standard methods)
    let api_cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    // Strict CORS policy for the health check (read-only)
    let health_cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET])
        .allow_headers(Any);

    let api_routes = Router::new()
        // Communities
        .route(
            "/v1/communities",
            get(handlers::communities::list).post(handlers::communities::create),
        )
        .route(
            "/v1/communities/{id}",
            get(handlers::communities::get)
                .put(handlers::communities::update)
                .delete(handlers::communities::delete),
        )
        // Locations
        .route(
            "/v1/communities/{community_id}/locations",
            get(handlers::locations::list).post(handlers::locations::create),
        )
        .route(
            "/v1/communities/{community_id}/locations/{id}",
            get(handlers::locations::get)
                .put(handlers::locations::update)
                .delete(handlers::locations::delete),
        )
        // Residents
        .route(
            "/v1/communities/{community_id}/residents",
            get(handlers::residents::list).post(handlers::residents::create),
        )
        .route(
            "/v1/communities/{community_id}/residents/{id}",
            get(handlers::residents::get)
                .put(handlers::residents::update)
                .delete(handlers::residents::delete),
        )
        .route(
            "/v1/communities/{community_id}/residents/{id}/photo",
            get(handlers::residents::get_photo)
                .put(handlers::residents::put_photo)
                .delete(handlers::residents::delete_photo),
        )
        .layer(api_cors);

    Router::new()
        .route("/v1/health", get(health_check).layer(health_cors))
        .merge(api_routes)
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
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // Endpoint handlers apply stricter limits where needed.
        .with_state(state)
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (axum::http::StatusCode::OK, Json(json!({"status": "ok"}))),
        Err(e) => {
            tracing::error!("Health check failed: database connectivity error: {:?}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": "database connection failed",
                    "reason": "database_connection_failed"
                })),
            )
        }
    }
}
