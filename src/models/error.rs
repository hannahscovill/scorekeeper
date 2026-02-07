//! Error types for the scorekeeper API.

use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Error response body matching OpenAPI spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

/// Inner error body with code and message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<ValidationDetail>>,
}

/// Validation error detail for 422 responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationDetail {
    pub field: String,
    pub message: String,
}

/// Application-level errors with standardized HTTP responses.
#[derive(Debug)]
pub enum AppError {
    /// 400 Bad Request
    BadRequest(String),
    /// 401 Unauthorized
    Unauthorized(String),
    /// 403 Forbidden
    Forbidden(String),
    /// 404 Not Found
    NotFound(String),
    /// 422 Validation Error
    ValidationError(Vec<ValidationDetail>),
    /// 429 Too Many Requests
    TooManyRequests(String),
    /// 500 Internal Server Error
    InternalError(String),
    /// 502 Bad Gateway
    BadGateway(String),
}

impl AppError {
    /// Create a 400 Bad Request error.
    pub fn bad_request(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }

    /// Create a 401 Unauthorized error with default message.
    pub fn unauthorized() -> Self {
        AppError::Unauthorized("Invalid or expired authentication token".to_string())
    }

    /// Create a 403 Forbidden error with default message.
    pub fn forbidden() -> Self {
        AppError::Forbidden("You do not have permission to access this resource".to_string())
    }

    /// Create a 404 Not Found error.
    pub fn not_found(resource: impl Into<String>) -> Self {
        AppError::NotFound(resource.into())
    }

    /// Create a 422 Validation Error.
    pub fn validation(details: Vec<ValidationDetail>) -> Self {
        AppError::ValidationError(details)
    }

    /// Create a 429 Too Many Requests error.
    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        AppError::TooManyRequests(msg.into())
    }

    /// Create a 500 Internal Server Error.
    pub fn internal(msg: impl Into<String>) -> Self {
        AppError::InternalError(msg.into())
    }

    /// Create a 502 Bad Gateway error.
    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        AppError::BadGateway(msg.into())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            AppError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            AppError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::ValidationError(_) => write!(f, "Validation failed"),
            AppError::TooManyRequests(msg) => write!(f, "Too many requests: {}", msg),
            AppError::InternalError(msg) => write!(f, "Internal server error: {}", msg),
            AppError::BadGateway(msg) => write!(f, "Bad gateway: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::BadRequest(msg) => HttpResponse::BadRequest().json(ErrorResponse {
                error: ErrorBody {
                    code: "BAD_REQUEST".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            }),
            AppError::Unauthorized(msg) => HttpResponse::Unauthorized().json(ErrorResponse {
                error: ErrorBody {
                    code: "UNAUTHORIZED".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            }),
            AppError::Forbidden(msg) => HttpResponse::Forbidden().json(ErrorResponse {
                error: ErrorBody {
                    code: "FORBIDDEN".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            }),
            AppError::NotFound(msg) => HttpResponse::NotFound().json(ErrorResponse {
                error: ErrorBody {
                    code: "NOT_FOUND".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            }),
            AppError::ValidationError(details) => {
                HttpResponse::UnprocessableEntity().json(ErrorResponse {
                    error: ErrorBody {
                        code: "VALIDATION_ERROR".to_string(),
                        message: "Validation failed".to_string(),
                        details: Some(details.clone()),
                    },
                })
            }
            AppError::TooManyRequests(msg) => HttpResponse::TooManyRequests().json(ErrorResponse {
                error: ErrorBody {
                    code: "TOO_MANY_REQUESTS".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            }),
            AppError::InternalError(msg) => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    error: ErrorBody {
                        code: "INTERNAL_ERROR".to_string(),
                        message: msg.clone(),
                        details: None,
                    },
                })
            }
            AppError::BadGateway(msg) => HttpResponse::BadGateway().json(ErrorResponse {
                error: ErrorBody {
                    code: "BAD_GATEWAY".to_string(),
                    message: msg.clone(),
                    details: None,
                },
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::body::to_bytes;
    use actix_web::http::StatusCode;

    #[test]
    fn test_app_error_display() {
        let err = AppError::NotFound("score".to_string());
        assert_eq!(err.to_string(), "Not found: score");
    }

    #[test]
    fn test_bad_request_helper() {
        let err = AppError::bad_request("Invalid request body");
        assert!(matches!(err, AppError::BadRequest(msg) if msg == "Invalid request body"));
    }

    #[test]
    fn test_unauthorized_helper() {
        let err = AppError::unauthorized();
        assert!(
            matches!(err, AppError::Unauthorized(msg) if msg == "Invalid or expired authentication token")
        );
    }

    #[test]
    fn test_forbidden_helper() {
        let err = AppError::forbidden();
        assert!(
            matches!(err, AppError::Forbidden(msg) if msg == "You do not have permission to access this resource")
        );
    }

    #[test]
    fn test_not_found_helper() {
        let err = AppError::not_found("Resource not found");
        assert!(matches!(err, AppError::NotFound(msg) if msg == "Resource not found"));
    }

    #[test]
    fn test_validation_helper() {
        let details = vec![ValidationDetail {
            field: "email".to_string(),
            message: "Invalid email format".to_string(),
        }];
        let err = AppError::validation(details.clone());
        assert!(matches!(err, AppError::ValidationError(d) if d.len() == 1));
    }

    #[actix_web::test]
    async fn test_bad_request_response() {
        let err = AppError::bad_request("Invalid request body");
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "BAD_REQUEST");
        assert_eq!(json["error"]["message"], "Invalid request body");
    }

    #[actix_web::test]
    async fn test_unauthorized_response() {
        let err = AppError::unauthorized();
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "UNAUTHORIZED");
        assert_eq!(
            json["error"]["message"],
            "Invalid or expired authentication token"
        );
    }

    #[actix_web::test]
    async fn test_forbidden_response() {
        let err = AppError::forbidden();
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "FORBIDDEN");
        assert_eq!(
            json["error"]["message"],
            "You do not have permission to access this resource"
        );
    }

    #[actix_web::test]
    async fn test_not_found_response() {
        let err = AppError::not_found("Resource not found");
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert_eq!(json["error"]["message"], "Resource not found");
    }

    #[actix_web::test]
    async fn test_validation_error_response() {
        let details = vec![
            ValidationDetail {
                field: "email".to_string(),
                message: "Invalid email format".to_string(),
            },
            ValidationDetail {
                field: "name".to_string(),
                message: "Name is required".to_string(),
            },
        ];
        let err = AppError::validation(details);
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(json["error"]["message"], "Validation failed");
        assert!(json["error"]["details"].is_array());
        assert_eq!(json["error"]["details"].as_array().unwrap().len(), 2);
        assert_eq!(json["error"]["details"][0]["field"], "email");
        assert_eq!(
            json["error"]["details"][0]["message"],
            "Invalid email format"
        );
    }

    #[actix_web::test]
    async fn test_internal_error_response() {
        let err = AppError::internal("Database connection failed");
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "INTERNAL_ERROR");
        assert_eq!(json["error"]["message"], "Database connection failed");
    }

    #[test]
    fn test_too_many_requests_helper() {
        let err = AppError::too_many_requests("Rate limit exceeded");
        assert!(matches!(err, AppError::TooManyRequests(msg) if msg == "Rate limit exceeded"));
    }

    #[test]
    fn test_bad_gateway_helper() {
        let err = AppError::bad_gateway("Upstream service failed");
        assert!(matches!(err, AppError::BadGateway(msg) if msg == "Upstream service failed"));
    }

    #[actix_web::test]
    async fn test_too_many_requests_response() {
        let err = AppError::too_many_requests("Rate limit exceeded");
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "TOO_MANY_REQUESTS");
        assert_eq!(json["error"]["message"], "Rate limit exceeded");
    }

    #[actix_web::test]
    async fn test_bad_gateway_response() {
        let err = AppError::bad_gateway("Failed to create issue");
        let response = err.error_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let body = to_bytes(response.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["error"]["code"], "BAD_GATEWAY");
        assert_eq!(json["error"]["message"], "Failed to create issue");
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse {
            error: ErrorBody {
                code: "BAD_REQUEST".to_string(),
                message: "Invalid input".to_string(),
                details: None,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("details")); // details should be skipped when None
        assert!(json.contains("BAD_REQUEST"));
        assert!(json.contains("Invalid input"));
    }

    #[test]
    fn test_validation_detail_serialization() {
        let detail = ValidationDetail {
            field: "score".to_string(),
            message: "Score must be positive".to_string(),
        };

        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("score"));
        assert!(json.contains("Score must be positive"));
    }
}
