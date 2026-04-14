use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub const ERR_FOREIGN_KEY_VIOLATION: &str = "23503";
pub const ERR_UNIQUE_VIOLATION: &str = "23505";
pub const REASON_INTERNAL_SERVER_ERROR: &str = "internal_server_error";

#[derive(Debug)]
pub enum AppError {
    Sqlx(sqlx::Error),
    InternalServer {
        message: String,
        reason: &'static str,
    },
    NotFound {
        message: String,
        reason: &'static str,
    },
    BadRequest {
        message: String,
        reason: &'static str,
    },
    PayloadTooLarge {
        message: String,
        reason: &'static str,
    },
    Conflict {
        message: String,
        reason: &'static str,
    },
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err)
    }
}

impl AppError {
    pub fn internal_server_error(reason: &'static str, message: impl Into<String>) -> Self {
        Self::InternalServer {
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

    pub fn bad_request(reason: &'static str, message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
            reason,
        }
    }

    pub fn payload_too_large(reason: &'static str, message: impl Into<String>) -> Self {
        Self::PayloadTooLarge {
            message: message.into(),
            reason,
        }
    }

    pub fn conflict(reason: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
            reason,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, reason) = match self {
            AppError::Sqlx(err) => {
                tracing::error!("Database error: {:?}", err);

                // Return a generic error message to the client to avoid leaking implementation details.
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal server error occurred.".to_string(),
                    REASON_INTERNAL_SERVER_ERROR,
                )
            }
            AppError::InternalServer { message, reason } => {
                (StatusCode::INTERNAL_SERVER_ERROR, message, reason)
            }
            AppError::NotFound { message, reason } => (StatusCode::NOT_FOUND, message, reason),
            AppError::BadRequest { message, reason } => (StatusCode::BAD_REQUEST, message, reason),
            AppError::PayloadTooLarge { message, reason } => {
                (StatusCode::PAYLOAD_TOO_LARGE, message, reason)
            }
            AppError::Conflict { message, reason } => (StatusCode::CONFLICT, message, reason),
        };

        (status, Json(json!({ "error": message, "reason": reason }))).into_response()
    }
}
