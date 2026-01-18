//! Scorekeeper API - Sports score tracking server.

use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use tracing::{info, warn};

pub mod config;
pub mod db;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod telemetry;

use config::Config;
use db::InMemoryDb;
use middleware::auth::JwtAuth;
use middleware::RequestTracing;
use routes::{create_scores, get_scores, health_check, list_scores};
use telemetry::{init_telemetry, shutdown_telemetry};

/// Initializes configuration by detecting the environment and using the appropriate secrets provider.
///
/// This function checks for the USE_AWS_SECRETS environment variable to determine whether to use
/// AWS Secrets Manager or fall back to environment variables. This allows for flexible deployment:
/// - In production (ECS/Fargate): Set USE_AWS_SECRETS=true to use AWS Secrets Manager
/// - In local development: Omit USE_AWS_SECRETS or set to false to use environment variables
async fn initialize_config() -> Config {
    #[cfg(feature = "aws-secrets")]
    {
        let use_aws_secrets = std::env::var("USE_AWS_SECRETS")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            == "true";

        let secret_name = std::env::var("AWS_SECRET_NAME").ok();

        if use_aws_secrets {
            info!("Initializing with AWS Secrets Manager");
            let aws_provider = AwsSecretsProvider::new().await;
            match Config::from_secrets(&aws_provider, secret_name.as_deref()).await {
                Ok(config) => {
                    info!("Configuration loaded from AWS Secrets Manager");
                    return config;
                }
                Err(e) => {
                    warn!("Failed to load config from AWS Secrets Manager: {}. Falling back to environment variables", e);
                }
            }
        }
    }

    // Use environment variable secrets provider
    info!("Initializing with environment variables");
    let secret_name = std::env::var("AWS_SECRET_NAME").ok();
    let env_provider = EnvSecretsProvider::new();
    match Config::from_secrets(&env_provider, secret_name.as_deref()).await {
        Ok(config) => {
            info!("Configuration loaded from environment variables");
            return config;
        }
        Err(e) => {
            warn!(
                "Failed to load config from secrets provider: {}. Using default from_env",
                e
            );
        }
    }

    // Final fallback to the original from_env method
    warn!("Using fallback configuration from environment variables");
    Config::from_env()
}

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello, World!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize distributed tracing with OpenTelemetry and AWS X-Ray
    // This must be done before any other tracing initialization
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "scorekeeper".to_string());
    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

    let _tracer_provider = match init_telemetry(service_name, otlp_endpoint) {
        Ok(provider) => {
            info!("OpenTelemetry initialized with AWS X-Ray export");
            Some(provider)
        }
        Err(e) => {
            warn!("Failed to initialize OpenTelemetry: {}. Continuing without distributed tracing.", e);
            // If telemetry fails, fall back to basic tracing
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("info".parse().unwrap()))
                .init();
            None
        }
    };

    // Initialize secrets provider based on environment
    let config = initialize_config().await;
    let bind_addr = config.bind_address();

    // Initialize shared state
    let db = web::Data::new(InMemoryDb::new());
    let jwt_auth = web::Data::new(JwtAuth::new(config.jwt_secret().to_string()));

    info!("Starting server at http://{}:{}", bind_addr.0, bind_addr.1);

    let server = HttpServer::new(move || {
        App::new()
            .wrap(RequestTracing) // Add tracing middleware for span propagation
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(hello)
            .service(health_check)
            .service(deep_health_check)
            .service(list_scores)
            .service(get_scores)
            .service(create_scores)
    })
    .bind(bind_addr)?
    .run();

    // Run the server
    let result = server.await;

    // Shutdown telemetry to flush remaining traces
    shutdown_telemetry();

    result
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
