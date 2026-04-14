use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest {
        message: String,
        reason: &'static str,
    },
    Unauthorized {
        message: String,
        reason: &'static str,
    },
    NotFound {
        message: String,
        reason: &'static str,
    },
    BadGateway {
        message: String,
        reason: &'static str,
    },
    InternalServer {
        message: String,
        reason: &'static str,
    },
}

impl AppError {
    pub fn bad_request(reason: &'static str, message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
            reason,
        }
    }

    pub fn unauthorized(reason: &'static str, message: impl Into<String>) -> Self {
        Self::Unauthorized {
            message: message.into(),
            reason,
        }
    }

    pub fn not_found(reason: &'static str, message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
            reason,
        }
    }

    pub fn bad_gateway(reason: &'static str, message: impl Into<String>) -> Self {
        Self::BadGateway {
            message: message.into(),
            reason,
        }
    }

    pub fn internal_server_error(reason: &'static str, message: impl Into<String>) -> Self {
        Self::InternalServer {
            message: message.into(),
            reason,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, reason) = match self {
            AppError::BadRequest { message, reason } => (StatusCode::BAD_REQUEST, message, reason),
            AppError::Unauthorized { message, reason } => {
                (StatusCode::UNAUTHORIZED, message, reason)
            }
            AppError::NotFound { message, reason } => (StatusCode::NOT_FOUND, message, reason),
            AppError::BadGateway { message, reason } => (StatusCode::BAD_GATEWAY, message, reason),
            AppError::InternalServer { message, reason } => {
                (StatusCode::INTERNAL_SERVER_ERROR, message, reason)
            }
        };

        (status, Json(json!({ "error": message, "reason": reason }))).into_response()
    }
}
