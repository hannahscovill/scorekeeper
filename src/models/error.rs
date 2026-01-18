//! Error types for the scorekeeper API.

use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// The body of an error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorBody {
    /// Error code identifier.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

impl ErrorBody {
    /// Creates a new error body.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Standard error response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    /// The error details.
    pub error: ErrorBody,
}

impl ErrorResponse {
    /// Creates a new error response.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ErrorBody::new(code, message),
        }
    }

    /// Creates a not found error response.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", message)
    }

    /// Creates a bad request error response.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new("BAD_REQUEST", message)
    }

    /// Creates an unauthorized error response.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new("UNAUTHORIZED", message)
    }

    /// Creates an internal server error response.
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new("INTERNAL_ERROR", message)
    }
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error.code, self.error.message)
    }
}

/// Details about a specific validation error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationDetail {
    /// The field that failed validation.
    pub field: String,
    /// Description of the validation error.
    pub message: String,
}

impl ValidationDetail {
    /// Creates a new validation detail.
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// The body of a validation error response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationErrorBody {
    /// Error code identifier.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Details about each validation error.
    pub details: Vec<ValidationDetail>,
}

impl ValidationErrorBody {
    /// Creates a new validation error body.
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Vec<ValidationDetail>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details,
        }
    }
}

/// Validation error response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationErrorResponse {
    /// The validation error details.
    pub error: ValidationErrorBody,
}

impl ValidationErrorResponse {
    /// Creates a new validation error response.
    pub fn new(message: impl Into<String>, details: Vec<ValidationDetail>) -> Self {
        Self {
            error: ValidationErrorBody::new("VALIDATION_ERROR", message, details),
        }
    }

    /// Creates a validation error response with a single detail.
    pub fn single(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            "Validation failed",
            vec![ValidationDetail::new(field, message)],
        )
    }
}

impl fmt::Display for ValidationErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} ({} errors)",
            self.error.code,
            self.error.message,
            self.error.details.len()
        )
    }
}

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

    /// Validation error with details.
    #[error("Validation error: {0}")]
    ValidationError(ValidationErrorResponse),
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::NotFound(msg) => HttpResponse::NotFound().json(ErrorResponse::not_found(msg)),
            AppError::BadRequest(msg) => {
                HttpResponse::BadRequest().json(ErrorResponse::bad_request(msg))
            }
            AppError::InternalError(msg) => {
                HttpResponse::InternalServerError().json(ErrorResponse::internal_error(msg))
            }
            AppError::Unauthorized(msg) => {
                HttpResponse::Unauthorized().json(ErrorResponse::unauthorized(msg))
            }
            AppError::ValidationError(validation_response) => {
                HttpResponse::BadRequest().json(validation_response)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_body_new() {
        let body = ErrorBody::new("TEST_CODE", "Test message");
        assert_eq!(body.code, "TEST_CODE");
        assert_eq!(body.message, "Test message");
    }

    #[test]
    fn test_error_response_new() {
        let response = ErrorResponse::new("TEST_CODE", "Test message");
        assert_eq!(response.error.code, "TEST_CODE");
        assert_eq!(response.error.message, "Test message");
    }

    #[test]
    fn test_error_response_helpers() {
        let not_found = ErrorResponse::not_found("Resource not found");
        assert_eq!(not_found.error.code, "NOT_FOUND");

        let bad_request = ErrorResponse::bad_request("Invalid input");
        assert_eq!(bad_request.error.code, "BAD_REQUEST");

        let unauthorized = ErrorResponse::unauthorized("Not authenticated");
        assert_eq!(unauthorized.error.code, "UNAUTHORIZED");

        let internal = ErrorResponse::internal_error("Something went wrong");
        assert_eq!(internal.error.code, "INTERNAL_ERROR");
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse::not_found("Score not found");
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"code\":\"NOT_FOUND\""));
        assert!(json.contains("\"message\":\"Score not found\""));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_error_response_deserialization() {
        let json = r#"{"error":{"code":"NOT_FOUND","message":"Score not found"}}"#;
        let response: ErrorResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.error.code, "NOT_FOUND");
        assert_eq!(response.error.message, "Score not found");
    }

    #[test]
    fn test_validation_detail_new() {
        let detail = ValidationDetail::new("email", "Invalid email format");
        assert_eq!(detail.field, "email");
        assert_eq!(detail.message, "Invalid email format");
    }

    #[test]
    fn test_validation_error_response_new() {
        let details = vec![
            ValidationDetail::new("score", "Score must be non-negative"),
            ValidationDetail::new("game_id", "Game ID is required"),
        ];
        let response = ValidationErrorResponse::new("Validation failed", details);

        assert_eq!(response.error.code, "VALIDATION_ERROR");
        assert_eq!(response.error.message, "Validation failed");
        assert_eq!(response.error.details.len(), 2);
    }

    #[test]
    fn test_validation_error_response_single() {
        let response = ValidationErrorResponse::single("score", "Score is required");

        assert_eq!(response.error.code, "VALIDATION_ERROR");
        assert_eq!(response.error.details.len(), 1);
        assert_eq!(response.error.details[0].field, "score");
    }

    #[test]
    fn test_validation_error_response_serialization() {
        let response = ValidationErrorResponse::single("score", "Score must be positive");
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"code\":\"VALIDATION_ERROR\""));
        assert!(json.contains("\"field\":\"score\""));
        assert!(json.contains("\"message\":\"Score must be positive\""));
        assert!(json.contains("\"details\""));
    }

    #[test]
    fn test_validation_error_response_deserialization() {
        let json = r#"{
            "error": {
                "code": "VALIDATION_ERROR",
                "message": "Validation failed",
                "details": [
                    {"field": "score", "message": "Score is required"}
                ]
            }
        }"#;
        let response: ValidationErrorResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.error.code, "VALIDATION_ERROR");
        assert_eq!(response.error.details.len(), 1);
        assert_eq!(response.error.details[0].field, "score");
    }

    #[test]
    fn test_app_error_display() {
        let err = AppError::NotFound("score".to_string());
        assert_eq!(err.to_string(), "Resource not found: score");

        let err = AppError::BadRequest("invalid input".to_string());
        assert_eq!(err.to_string(), "Bad request: invalid input");
    }

    #[test]
    fn test_error_response_display() {
        let response = ErrorResponse::not_found("Score not found");
        assert_eq!(response.to_string(), "NOT_FOUND: Score not found");
    }

    #[test]
    fn test_validation_error_response_display() {
        let details = vec![
            ValidationDetail::new("field1", "error1"),
            ValidationDetail::new("field2", "error2"),
        ];
        let response = ValidationErrorResponse::new("Validation failed", details);
        assert_eq!(
            response.to_string(),
            "VALIDATION_ERROR: Validation failed (2 errors)"
        );
    }
}
