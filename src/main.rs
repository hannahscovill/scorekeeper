//! Scorekeeper API - Sports score tracking server.

use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub mod config;
pub mod db;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;

use config::Config;
use db::InMemoryDb;
use middleware::auth::JwtAuth;
use routes::{create_scores, get_scores, health_check, list_scores};

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello, World!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let config = Config::from_env();
    let bind_addr = config.bind_address();

    // Initialize shared state
    let db = web::Data::new(InMemoryDb::new());
    let jwt_auth = web::Data::new(JwtAuth::new(config.jwt_secret().to_string()));

    info!("Starting server at http://{}:{}", bind_addr.0, bind_addr.1);

    HttpServer::new(move || {
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(hello)
            .service(health_check)
            .service(list_scores)
            .service(create_scores)
            .service(get_scores)
    })
    .bind(bind_addr)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_hello_endpoint() {
        let app = test::init_service(App::new().service(hello)).await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        assert_eq!(body, "Hello, World!");
    }

    #[actix_web::test]
    async fn test_health_endpoint() {
        let app = test::init_service(App::new().service(health_check)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let body = test::read_body(resp).await;
        assert_eq!(body, "OK");
    }
}
