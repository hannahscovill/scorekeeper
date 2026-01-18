//! Health check endpoint handlers.

use actix_web::{get, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::timeout;

use crate::db::InMemoryDb;

/// Health check endpoint.
///
/// Returns a 200 OK response if the server is healthy.
#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

/// Component health status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
}

/// Individual component health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Deep health check response containing all component statuses.
#[derive(Debug, Serialize, Deserialize)]
pub struct DeepHealthResponse {
    pub status: HealthStatus,
    pub components: ComponentHealthDetails,
}

/// Individual component health details.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentHealthDetails {
    pub database: ComponentHealth,
}

/// Deep health check endpoint that verifies all critical dependencies.
///
/// Checks database connectivity and returns structured JSON with detailed status.
/// Returns 200 if all components are healthy, 503 if any are unhealthy.
#[get("/health/deep")]
pub async fn deep_health_check(db: web::Data<InMemoryDb>) -> impl Responder {
    let db_health = check_database_health(&db).await;

    let overall_status = if db_health.status == HealthStatus::Healthy {
        HealthStatus::Healthy
    } else {
        HealthStatus::Unhealthy
    };

    let response = DeepHealthResponse {
        status: overall_status.clone(),
        components: ComponentHealthDetails {
            database: db_health,
        },
    };

    match overall_status {
        HealthStatus::Healthy => HttpResponse::Ok().json(response),
        HealthStatus::Unhealthy => HttpResponse::ServiceUnavailable().json(response),
    }
}

/// Check database health by attempting to acquire a read lock.
/// Times out after 1 second to prevent hanging.
async fn check_database_health(db: &InMemoryDb) -> ComponentHealth {
    // Timeout for database health check (1 second)
    const DB_HEALTH_TIMEOUT: Duration = Duration::from_secs(1);

    let db_check = async {
        // Verify we can get all scores (requires read lock)
        match db.get_all_scores() {
            Ok(_) => ComponentHealth {
                status: HealthStatus::Healthy,
                message: None,
            },
            Err(e) => ComponentHealth {
                status: HealthStatus::Unhealthy,
                message: Some(format!("Database error: {}", e)),
            },
        }
    };

    match timeout(DB_HEALTH_TIMEOUT, db_check).await {
        Ok(result) => result,
        Err(_) => ComponentHealth {
            status: HealthStatus::Unhealthy,
            message: Some("Database health check timeout".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};

    #[actix_web::test]
    async fn test_health_check() {
        let app = test::init_service(App::new().service(health_check)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        assert_eq!(body, "OK");
    }

    #[actix_web::test]
    async fn test_deep_health_check_healthy() {
        let db = web::Data::new(InMemoryDb::new());
        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .service(deep_health_check),
        )
        .await;

        let req = test::TestRequest::get().uri("/health/deep").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        let response: DeepHealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(response.status, HealthStatus::Healthy);
        assert_eq!(response.components.database.status, HealthStatus::Healthy);
        assert!(response.components.database.message.is_none());
    }

    #[tokio::test]
    async fn test_component_health_serialization() {
        let health = ComponentHealth {
            status: HealthStatus::Healthy,
            message: None,
        };

        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"status\":\"healthy\""));
        assert!(!json.contains("message")); // Should be omitted when None
    }

    #[tokio::test]
    async fn test_component_health_with_message_serialization() {
        let health = ComponentHealth {
            status: HealthStatus::Unhealthy,
            message: Some("Connection failed".to_string()),
        };

        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"status\":\"unhealthy\""));
        assert!(json.contains("\"message\":\"Connection failed\""));
    }

    #[tokio::test]
    async fn test_deep_health_response_serialization() {
        let response = DeepHealthResponse {
            status: HealthStatus::Healthy,
            components: ComponentHealthDetails {
                database: ComponentHealth {
                    status: HealthStatus::Healthy,
                    message: None,
                },
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"healthy\""));
        assert!(json.contains("\"components\""));
        assert!(json.contains("\"database\""));
    }
}
