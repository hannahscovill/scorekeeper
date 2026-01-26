//! Integration tests for the Scorekeeper API
//!
//! NOTE: These tests were removed during Auth0 migration.
//! The API now uses Auth0 RS256 JWTs which require JWKS validation.
//! Integration tests would need to use real Auth0 tokens or mock the JWKS endpoint.

use actix_web::{test, App};
use scorekeeper::routes::health_check;

#[actix_web::test]
async fn test_health_endpoint() {
    let app = test::init_service(App::new().service(health_check)).await;
    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
}
