use crate::error::AppError;

pub mod communities;
pub mod locations;
pub mod residents;

const MAX_NAME_LENGTH: usize = 1000;

pub fn validate_name(name: Option<String>) -> Result<String, AppError> {
    match name {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                Err(AppError::bad_request("name_empty", "Name cannot be empty"))
            } else if trimmed.len() > MAX_NAME_LENGTH {
                Err(AppError::bad_request(
                    "name_too_long",
                    format!(
                        "Name exceeds maximum length of {} characters",
                        MAX_NAME_LENGTH
                    ),
                ))
            } else {
                Ok(trimmed.to_string())
            }
        }
        None => Err(AppError::bad_request("name_required", "Name is required")),
    }
}

pub fn validate_first_name(first_name: Option<String>) -> Result<String, AppError> {
    match first_name {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.len() > MAX_NAME_LENGTH {
                Err(AppError::bad_request(
                    "first_name_too_long",
                    format!(
                        "First name exceeds maximum length of {} characters",
                        MAX_NAME_LENGTH
                    ),
                ))
            } else {
                Ok(trimmed.to_string())
            }
        }
        None => Err(AppError::bad_request(
            "first_name_required",
            "First name is required",
        )),
    }
}

pub fn validate_last_name(last_name: Option<String>) -> Result<String, AppError> {
    match last_name {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(AppError::bad_request(
                    "last_name_empty",
                    "Last name cannot be empty",
                ))
            } else if trimmed.len() > MAX_NAME_LENGTH {
                Err(AppError::bad_request(
                    "last_name_too_long",
                    format!(
                        "Last name exceeds maximum length of {} characters",
                        MAX_NAME_LENGTH
                    ),
                ))
            } else {
                Ok(trimmed.to_string())
            }
        }
        None => Err(AppError::bad_request(
            "last_name_required",
            "Last name is required",
        )),
    }
}
