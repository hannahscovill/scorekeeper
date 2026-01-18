//! Error types for the scorekeeper API.

use actix_web::{HttpResponse, ResponseError};
use thiserror::Error;

/// Application-level errors.
#[derive(Debug, Error)]
pub enum AppError {
    /// Resource not found.
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Bad request with validation error.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Internal server error.
    #[error("Internal server error: {0}")]
    InternalError(String),

    /// Unauthorized access.
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::NotFound(msg) => {
                HttpResponse::NotFound().json(serde_json::json!({ "error": msg }))
            }
            AppError::BadRequest(msg) => {
                HttpResponse::BadRequest().json(serde_json::json!({ "error": msg }))
            }
            AppError::InternalError(msg) => {
                HttpResponse::InternalServerError().json(serde_json::json!({ "error": msg }))
            }
            AppError::Unauthorized(msg) => {
                HttpResponse::Unauthorized().json(serde_json::json!({ "error": msg }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::NotFound("score".to_string());
        assert_eq!(err.to_string(), "Resource not found: score");
    }
}
