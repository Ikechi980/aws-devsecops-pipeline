use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    connected_clients: usize,
}

pub async fn handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        connected_clients: state.client_count(),
    })
}
