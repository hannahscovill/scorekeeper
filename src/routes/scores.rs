//! Score-related route handlers.

use actix_web::{get, HttpResponse, Responder};

/// Placeholder endpoint for listing scores.
#[get("/scores")]
pub async fn list_scores() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "scores": [] }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_list_scores() {
        let app = test::init_service(App::new().service(list_scores)).await;
        let req = test::TestRequest::get().uri("/scores").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }
}
