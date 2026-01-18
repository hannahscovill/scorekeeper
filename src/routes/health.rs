//! Health check endpoint handlers.

use actix_web::{get, HttpResponse, Responder};

/// Health check endpoint.
///
/// Returns a 200 OK response if the server is healthy.
#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_health_check() {
        let app = test::init_service(App::new().service(health_check)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        assert_eq!(body, "OK");
    }
}
