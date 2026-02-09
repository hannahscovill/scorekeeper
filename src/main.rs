//! Scorekeeper API - Sports score tracking server.

use actix_cors::Cors;
use actix_web::{get, http::header, web, App, HttpResponse, HttpServer, Responder};
use rustls::ServerConfig;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tracing::info;

pub mod config;
pub mod db;
pub mod dictionary;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod telemetry;

use config::{Config, Environment};
use db::{
    DynamoDbPuzzleRepository, DynamoDbRepository, GameDatabase, InMemoryDb, InMemoryPuzzleDb,
    PuzzleDatabase,
};
use middleware::auth::JwtAuth;
use routes::{
    clear_puzzle_cache, create_games, create_issue, get_game, get_games, get_history, get_profile,
    get_puzzle_by_date, get_puzzles, health_check, list_games, set_puzzle, submit_guess,
    update_profile, upload_avatar,
};
use services::{
    Auth0ManagementService, CommonWordsService, CommonWordsSource, GitHubIssueService, RateLimiter,
    S3AvatarService,
};

/// Load TLS configuration from certificate and key files.
fn load_tls_config(cert_path: &str, key_path: &str) -> std::io::Result<ServerConfig> {
    let cert_file = File::open(cert_path)
        .map_err(|e| std::io::Error::new(e.kind(), format!("Failed to open cert file: {}", e)))?;
    let key_file = File::open(key_path)
        .map_err(|e| std::io::Error::new(e.kind(), format!("Failed to open key file: {}", e)))?;

    let cert_reader = &mut BufReader::new(cert_file);
    let key_reader = &mut BufReader::new(key_file);

    let certs: Vec<_> = rustls_pemfile::certs(cert_reader)
        .filter_map(|c| c.ok())
        .collect();

    let keys: Vec<_> = rustls_pemfile::pkcs8_private_keys(key_reader)
        .filter_map(|k| k.ok())
        .collect();

    if certs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "No certificates found in cert file",
        ));
    }

    if keys.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "No private keys found in key file",
        ));
    }

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, keys.into_iter().next().unwrap().into())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    Ok(config)
}

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello, World!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let environment = Environment::from_env();

    // Initialize telemetry (OpenTelemetry + tracing)
    telemetry::init_telemetry(&environment);

    let config = Config::from_env();
    let bind_addr = config.bind_address();

    // Initialize database - use DynamoDB if configured, otherwise in-memory
    let (db, puzzle_db): (Arc<dyn GameDatabase>, Arc<dyn PuzzleDatabase>) =
        if let Some(table_name) = config.dynamodb_table_name() {
            info!("Initializing DynamoDB client for table: {}", table_name);
            let sdk_config = if let Some(endpoint_url) = config.dynamodb_endpoint_url() {
                info!("Using DynamoDB endpoint: {}", endpoint_url);
                aws_config::from_env()
                    .endpoint_url(endpoint_url)
                    .load()
                    .await
            } else {
                aws_config::load_from_env().await
            };
            let client = aws_sdk_dynamodb::Client::new(&sdk_config);
            (
                Arc::new(DynamoDbRepository::new(
                    client.clone(),
                    table_name.to_string(),
                )),
                Arc::new(DynamoDbPuzzleRepository::new(
                    client,
                    table_name.to_string(),
                )),
            )
        } else {
            info!("Using in-memory database");
            (
                Arc::new(InMemoryDb::new()),
                Arc::new(InMemoryPuzzleDb::new()),
            )
        };
    let db = web::Data::new(db);
    let puzzle_db = web::Data::new(puzzle_db);

    // Initialize other shared state
    let jwt_auth = web::Data::new(JwtAuth::new(
        config.auth0_domain().to_string(),
        config.auth0_audience().to_string(),
    ));

    // Initialize Auth0 Management API service (required for profile endpoints)
    let auth0_service = match (
        config.auth0_m2m_client_id(),
        config.auth0_m2m_client_secret(),
    ) {
        (Some(client_id), Some(client_secret)) => {
            info!("Auth0 Management API service enabled");
            Some(web::Data::new(Auth0ManagementService::new(
                config.auth0_domain().to_string(),
                client_id.to_string(),
                client_secret.to_string(),
            )))
        }
        _ => {
            info!("Auth0 Management API service disabled (missing M2M credentials)");
            info!("Profile endpoints will not be available");
            None
        }
    };

    // Initialize S3 Avatar service (requires both S3 bucket and Auth0 M2M)
    let s3_avatar_service = if let Some(bucket) = config.s3_avatar_bucket() {
        info!("S3 Avatar service enabled for bucket: {}", bucket);
        let sdk_config = aws_config::load_from_env().await;
        let s3_client = aws_sdk_s3::Client::new(&sdk_config);
        Some(web::Data::new(S3AvatarService::new(
            s3_client,
            bucket.to_string(),
        )))
    } else {
        info!("S3 Avatar service disabled (S3_AVATAR_BUCKET not configured)");
        None
    };

    // Initialize Common Words service for puzzle word selection
    // Prefer local file path (for development), fall back to S3 (for production)
    let common_words_service = if let Some(file_path) = config.common_words_file_path() {
        info!("Common Words service using local file: {}", file_path);
        let service = CommonWordsService::new(CommonWordsSource::File(file_path.to_string()));

        if let Err(e) = service.load().await {
            tracing::error!("Failed to load common words from file: {}", e);
            tracing::error!("Random puzzle word selection will not work!");
        }

        Some(web::Data::new(service))
    } else if let Some(bucket) = config.s3_common_words_bucket() {
        let key = config.s3_common_words_key().to_string();
        info!("Common Words service using S3: s3://{}/{}", bucket, key);
        let sdk_config = aws_config::load_from_env().await;
        let s3_client = aws_sdk_s3::Client::new(&sdk_config);
        let service = CommonWordsService::new(CommonWordsSource::S3 {
            client: s3_client,
            bucket: bucket.to_string(),
            key,
        });

        if let Err(e) = service.load().await {
            tracing::error!("Failed to load common words from S3: {}", e);
            tracing::error!("Random puzzle word selection will not work!");
        }

        Some(web::Data::new(service))
    } else {
        info!("Common Words service disabled (no file path or S3 bucket configured)");
        info!("Random puzzle word selection will not be available");
        None
    };

    // Initialize GitHub Issue service (requires GitHub App credentials or PAT)
    // Validate that credentials are complete — fail fast on partial config.
    let has_app_id = config.github_app_id().is_some();
    let has_installation_id = config.github_installation_id().is_some();
    let has_private_key = config.github_private_key().is_some();
    let has_token = config.github_token().is_some();
    let app_creds = [has_app_id, has_installation_id, has_private_key];
    let some_app_creds = app_creds.iter().any(|&v| v);
    let all_app_creds = app_creds.iter().all(|&v| v);

    if some_app_creds && !all_app_creds && !has_token {
        let missing: Vec<&str> = [
            (!has_app_id, "GITHUB_APP_ID"),
            (!has_installation_id, "GITHUB_INSTALLATION_ID"),
            (
                !has_private_key,
                "GITHUB_PRIVATE_KEY / GITHUB_PRIVATE_KEY_FILE",
            ),
        ]
        .iter()
        .filter(|(m, _)| *m)
        .map(|(_, name)| *name)
        .collect();
        panic!(
            "Partial GitHub App config detected. Missing: {}. \
             Set all three (GITHUB_APP_ID, GITHUB_INSTALLATION_ID, GITHUB_PRIVATE_KEY) \
             or remove them all to disable the issue proxy.",
            missing.join(", ")
        );
    }

    // In production, fail fast if issue proxy credentials are missing entirely.
    if environment.is_production() && !all_app_creds && !has_token {
        panic!(
            "ENVIRONMENT=production but GitHub issue proxy credentials are not configured. \
             Set GITHUB_APP_ID, GITHUB_INSTALLATION_ID, and GITHUB_PRIVATE_KEY \
             (or GITHUB_TOKEN for PAT auth)."
        );
    }

    let github_issue_service = if all_app_creds || has_token {
        info!("GitHub Issue service enabled");
        Some(web::Data::new(GitHubIssueService::new(
            config.github_app_id().map(|s| s.to_string()),
            config.github_installation_id().map(|s| s.to_string()),
            config.github_private_key().map(|s| s.to_string()),
            config.github_token().map(|s| s.to_string()),
            config.github_repo().to_string(),
            config.turnstile_secret_key().map(|s| s.to_string()),
            config.turnstile_verify_url().to_string(),
            std::env::var("COMMIT_HASH").ok(),
        )))
    } else {
        info!("GitHub Issue service disabled (no GitHub credentials configured)");
        None
    };

    let issue_rate_limiter = web::Data::new(RateLimiter::new());

    let config_for_tls = config.clone();
    let config = web::Data::new(config);

    // Clone CORS origins for use in closure
    let cors_origins = config.cors_allowed_origins().to_vec();
    info!("CORS allowed origins: {:?}", cors_origins);

    let server = HttpServer::new(move || {
        // Configure CORS with allowed origins from config
        let mut cors = Cors::default()
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
            .allowed_headers(vec![
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::ORIGIN,
                // W3C Trace Context headers sent by the frontend's OpenTelemetry
                // FetchInstrumentation. Without these the CORS preflight rejects
                // credentialed requests that carry trace propagation headers.
                header::HeaderName::from_static("traceparent"),
                header::HeaderName::from_static("tracestate"),
            ])
            .expose_headers(vec![header::CONTENT_LENGTH, header::CONTENT_TYPE])
            .supports_credentials()
            .max_age(3600);

        // Add each allowed origin
        for origin in &cors_origins {
            cors = cors.allowed_origin(origin);
        }

        let mut app = App::new()
            .wrap(cors)
            .app_data(db.clone())
            .app_data(puzzle_db.clone())
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(hello)
            .service(health_check)
            .service(list_games)
            .service(get_games)
            .service(get_game)
            .service(create_games)
            .service(submit_guess)
            .service(get_puzzle_by_date)
            .service(get_puzzles)
            .service(set_puzzle)
            .service(clear_puzzle_cache)
            .service(get_history);

        // Add common words service if configured
        if let Some(ref cws) = common_words_service {
            app = app.app_data(cws.clone());
        }

        // Only register issue endpoint if GitHub credentials are configured
        if let Some(ref gh) = github_issue_service {
            app = app
                .app_data(gh.clone())
                .app_data(issue_rate_limiter.clone())
                .service(create_issue);
        }

        // Only register profile endpoints if Auth0 M2M is configured
        if let Some(ref auth0) = auth0_service {
            app = app.app_data(auth0.clone());
            if let Some(ref s3) = s3_avatar_service {
                app = app.app_data(s3.clone());
            }
            app = app
                .service(get_profile)
                .service(update_profile);
            if s3_avatar_service.is_some() {
                app = app.service(upload_avatar);
            }
        }

        app
    });

    // Bind with TLS if enabled, otherwise plain HTTP
    let result = if config_for_tls.tls_enabled() {
        let cert_path = config_for_tls.tls_cert_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "TLS_CERT_PATH is required when TLS_ENABLED=true",
            )
        })?;
        let key_path = config_for_tls.tls_key_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "TLS_KEY_PATH is required when TLS_ENABLED=true",
            )
        })?;

        let tls_config = load_tls_config(cert_path, key_path)?;
        info!("Starting server at https://{}:{}", bind_addr.0, bind_addr.1);
        server.bind_rustls_0_23(bind_addr, tls_config)?.run().await
    } else {
        info!("Starting server at http://{}:{}", bind_addr.0, bind_addr.1);
        server.bind(bind_addr)?.run().await
    };

    // Graceful shutdown - flush pending spans
    telemetry::shutdown_telemetry();

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
