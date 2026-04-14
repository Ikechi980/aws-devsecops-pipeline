use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub enum AppError {
    BadRequest(String),
    Forbidden(String),
    NotFound(String),
    BadGateway(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadGateway(msg) => (StatusCode::BAD_GATEWAY, msg),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
