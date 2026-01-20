//! Integration tests for the Scorekeeper API
//!
//! These tests verify full API request/response cycles, including:
//! - Game submission and retrieval
//! - Team filtering across endpoints
//! - Authentication scenarios
//! - Validation error responses

use actix_web::{http::StatusCode, test, web, App};
use jsonwebtoken::{encode, EncodingKey, Header};
use scorekeeper::config::Config;
use scorekeeper::db::{GameDatabase, InMemoryDb};
use scorekeeper::middleware::auth::{Claims, JwtAuth};
use scorekeeper::models::error::ErrorResponse;
use scorekeeper::models::game::{Game, GameCreate};
use scorekeeper::routes::{create_games, get_games, health_check};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const TEST_JWT_SECRET: &str = "integration-test-secret-key";

/// Helper function to get the current Unix timestamp.
fn get_current_timestamp() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

/// Generate a valid JWT token for testing.
fn generate_test_token(secret: &str, user_id: Uuid, team_id: Option<Uuid>) -> String {
    let now = get_current_timestamp();
    let claims = Claims {
        sub: user_id,
        exp: now + 3600, // 1 hour from now
        iat: now,
        team_id,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

/// Generate an expired JWT token for testing.
fn generate_expired_token(secret: &str, user_id: Uuid) -> String {
    let now = get_current_timestamp();
    let claims = Claims {
        sub: user_id,
        exp: now - 3600, // expired 1 hour ago
        iat: now - 7200,
        team_id: None,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

/// Create shared database, JWT auth, and config for tests.
fn create_test_data() -> (Arc<InMemoryDb>, web::Data<JwtAuth>, web::Data<Config>) {
    let db = Arc::new(InMemoryDb::new());
    let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));
    let config = web::Data::new(Config {
        host: "0.0.0.0".to_string(),
        port: 8080,
        database_url: None,
        jwt_secret: TEST_JWT_SECRET.to_string(),
        bypass_auth: false,
        tls_enabled: false,
        tls_cert_path: None,
        tls_key_path: None,
        dynamodb_endpoint_url: None,
        dynamodb_table_name: None,
    });
    (db, jwt_auth, config)
}

/// Convert InMemoryDb Arc to trait object for app data
fn db_to_app_data(db: Arc<InMemoryDb>) -> web::Data<Arc<dyn GameDatabase>> {
    let db: Arc<dyn GameDatabase> = db;
    web::Data::new(db)
}

// ==================== Full Game Submission and Retrieval Cycle ====================

#[actix_web::test]
async fn test_full_game_submission_and_retrieval_cycle() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(health_check)
            .service(get_games)
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, Some(team_id));

    // Step 1: POST games
    let games_to_create = vec![
        GameCreate::with_game(100, game_id),
        GameCreate::with_game(200, game_id),
        GameCreate::with_game(300, game_id),
    ];

    let post_req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&games_to_create)
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::CREATED);

    let created_games: Vec<Game> = test::read_body_json(post_resp).await;
    assert_eq!(created_games.len(), 3);

    // Verify created games have correct values
    assert_eq!(created_games[0].score, 100);
    assert_eq!(created_games[1].score, 200);
    assert_eq!(created_games[2].score, 300);

    // Verify all games belong to the same user and team
    for game in &created_games {
        assert_eq!(game.user_id, user_id);
        assert_eq!(game.team_id, Some(team_id));
        assert_eq!(game.game_id, game_id);
    }

    // Step 2: GET games for the game session
    let get_req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);

    let retrieved_games: Vec<Game> = test::read_body_json(get_resp).await;
    assert_eq!(retrieved_games.len(), 3);

    // Verify we can find all the created games
    let score_values: Vec<i32> = retrieved_games.iter().map(|g| g.score).collect();
    assert!(score_values.contains(&100));
    assert!(score_values.contains(&200));
    assert!(score_values.contains(&300));
}

#[actix_web::test]
async fn test_game_submission_generates_unique_ids() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    let games_to_create = vec![GameCreate::new(100), GameCreate::new(200)];

    let req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&games_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_games: Vec<Game> = test::read_body_json(resp).await;
    assert_eq!(created_games.len(), 2);

    // Each game should have a unique ID
    assert_ne!(created_games[0].id, created_games[1].id);
    // Each game without game_id should get a unique game_id
    assert_ne!(created_games[0].game_id, created_games[1].game_id);
}

// ==================== Team Filtering Across Endpoints ====================

#[actix_web::test]
async fn test_team_filtering_across_endpoints() {
    let (db, jwt_auth, config) = create_test_data();

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let team_a = Uuid::new_v4();
    let team_b = Uuid::new_v4();

    // Pre-populate database with games for different teams
    db.insert_game(Game::with_team(user_id, game_id, team_a, 100))
        .unwrap();
    db.insert_game(Game::with_team(user_id, game_id, team_a, 150))
        .unwrap();
    db.insert_game(Game::with_team(user_id, game_id, team_b, 200))
        .unwrap();
    db.insert_game(Game::with_team(user_id, game_id, team_b, 250))
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Get games filtered by team A
    let req_team_a = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", team_a.to_string()))
        .to_request();

    let resp_team_a = test::call_service(&app, req_team_a).await;
    assert_eq!(resp_team_a.status(), StatusCode::OK);

    let games_team_a: Vec<Game> = test::read_body_json(resp_team_a).await;
    assert_eq!(games_team_a.len(), 2);
    for game in &games_team_a {
        assert_eq!(game.team_id, Some(team_a));
    }

    // Get games filtered by team B
    let req_team_b = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", team_b.to_string()))
        .to_request();

    let resp_team_b = test::call_service(&app, req_team_b).await;
    assert_eq!(resp_team_b.status(), StatusCode::OK);

    let games_team_b: Vec<Game> = test::read_body_json(resp_team_b).await;
    assert_eq!(games_team_b.len(), 2);
    for game in &games_team_b {
        assert_eq!(game.team_id, Some(team_b));
    }

    // Get all games (no team filter)
    let req_all = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp_all = test::call_service(&app, req_all).await;
    assert_eq!(resp_all.status(), StatusCode::OK);

    let all_games: Vec<Game> = test::read_body_json(resp_all).await;
    assert_eq!(all_games.len(), 4);
}

#[actix_web::test]
async fn test_team_filtering_with_nonexistent_team() {
    let (db, jwt_auth, config) = create_test_data();

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let nonexistent_team = Uuid::new_v4();

    // Insert some games
    db.insert_game(Game::with_team(user_id, game_id, team_id, 100))
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Filter by a team that has no games
    let req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", nonexistent_team.to_string()))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let games: Vec<Game> = test::read_body_json(resp).await;
    assert!(games.is_empty());
}

// ==================== Unauthorized Access Tests ====================

#[actix_web::test]
async fn test_unauthorized_access_both_endpoints() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games)
            .service(create_games),
    )
    .await;

    let game_id = Uuid::new_v4();

    // Test GET without token
    let get_req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::UNAUTHORIZED);

    let get_body: ErrorResponse = test::read_body_json(get_resp).await;
    assert_eq!(get_body.error.code, "UNAUTHORIZED");

    // Test POST without token
    let post_req = test::TestRequest::post()
        .uri("/games")
        .set_json(&vec![GameCreate::new(100)])
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::UNAUTHORIZED);

    let post_body: ErrorResponse = test::read_body_json(post_resp).await;
    assert_eq!(post_body.error.code, "UNAUTHORIZED");
}

#[actix_web::test]
async fn test_unauthorized_with_invalid_token() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games)
            .service(create_games),
    )
    .await;

    let game_id = Uuid::new_v4();

    // Test GET with invalid token
    let get_req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", "Bearer invalid-token-here"))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::UNAUTHORIZED);

    // Test POST with invalid token
    let post_req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", "Bearer invalid-token-here"))
        .set_json(&vec![GameCreate::new(100)])
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_unauthorized_with_expired_token() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games)
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let expired_token = generate_expired_token(TEST_JWT_SECRET, user_id);

    // Test GET with expired token
    let get_req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", expired_token)))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::UNAUTHORIZED);

    // Test POST with expired token
    let post_req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", expired_token)))
        .set_json(&vec![GameCreate::new(100)])
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_unauthorized_with_wrong_secret() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    // Token signed with different secret
    let wrong_secret_token = generate_test_token("wrong-secret", user_id, None);

    let req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", wrong_secret_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================== Validation Error Response Tests ====================

#[actix_web::test]
async fn test_validation_error_responses() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test 422 - Empty game list
    let empty_games: Vec<GameCreate> = vec![];
    let req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&empty_games)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: ErrorResponse = test::read_body_json(resp).await;
    assert_eq!(body.error.code, "VALIDATION_ERROR");
    assert!(body.error.details.is_some());
    let details = body.error.details.unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].field, "games");
}

#[actix_web::test]
async fn test_bad_request_invalid_game_id() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test 400 - Invalid game_id format
    let req = test::TestRequest::get()
        .uri("/games/not-a-valid-uuid")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body: ErrorResponse = test::read_body_json(resp).await;
    assert_eq!(body.error.code, "BAD_REQUEST");
    assert!(body.error.message.contains("UUID"));
}

#[actix_web::test]
async fn test_bad_request_invalid_team_id_header() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test 400 - Invalid team-id header format
    let req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", "not-a-uuid"))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body: ErrorResponse = test::read_body_json(resp).await;
    assert_eq!(body.error.code, "BAD_REQUEST");
    assert!(body.error.message.contains("team-id"));
}

// ==================== Health Endpoint Tests ====================

#[actix_web::test]
async fn test_health_endpoint() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .service(health_check),
    )
    .await;

    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    assert_eq!(body, "OK");
}

#[actix_web::test]
async fn test_health_endpoint_no_auth_required() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .service(health_check),
    )
    .await;

    // Health endpoint should work without authentication
    let req = test::TestRequest::get().uri("/health").to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

// ==================== Edge Cases and Additional Scenarios ====================

#[actix_web::test]
async fn test_get_games_empty_game_session() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Get games for a game session with no games
    let req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let games: Vec<Game> = test::read_body_json(resp).await;
    assert!(games.is_empty());
}

#[actix_web::test]
async fn test_score_values_preserved_correctly() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test various score values including negative and zero
    let games_to_create = vec![
        GameCreate::with_game(-100, game_id),
        GameCreate::with_game(0, game_id),
        GameCreate::with_game(999999, game_id),
    ];

    let req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&games_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_games: Vec<Game> = test::read_body_json(resp).await;
    let score_values: Vec<i32> = created_games.iter().map(|g| g.score).collect();

    assert!(score_values.contains(&-100));
    assert!(score_values.contains(&0));
    assert!(score_values.contains(&999999));
}

#[actix_web::test]
async fn test_multiple_users_same_game_session() {
    let (db, jwt_auth, config) = create_test_data();

    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let game_id = Uuid::new_v4();

    // Insert games from different users
    db.insert_game(Game::new(user_a, game_id, 100)).unwrap();
    db.insert_game(Game::new(user_b, game_id, 200)).unwrap();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(get_games),
    )
    .await;

    let token = generate_test_token(TEST_JWT_SECRET, user_a, None);

    // Get all games for the game session
    let req = test::TestRequest::get()
        .uri(&format!("/games/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let games: Vec<Game> = test::read_body_json(resp).await;
    assert_eq!(games.len(), 2);

    // Verify both users' games are returned
    let user_ids: Vec<Uuid> = games.iter().map(|g| g.user_id).collect();
    assert!(user_ids.contains(&user_a));
    assert!(user_ids.contains(&user_b));
}

#[actix_web::test]
async fn test_jwt_claims_user_id_used_for_created_games() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, Some(team_id));

    let games_to_create = vec![GameCreate::new(100)];

    let req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&games_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_games: Vec<Game> = test::read_body_json(resp).await;
    assert_eq!(created_games.len(), 1);

    // The user_id and team_id should come from the JWT claims
    assert_eq!(created_games[0].user_id, user_id);
    assert_eq!(created_games[0].team_id, Some(team_id));
}

#[actix_web::test]
async fn test_jwt_without_team_id_creates_games_without_team() {
    let (db, jwt_auth, config) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db_to_app_data(db.clone()))
            .app_data(jwt_auth.clone())
            .app_data(config.clone())
            .service(create_games),
    )
    .await;

    let user_id = Uuid::new_v4();
    // Token without team_id
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    let games_to_create = vec![GameCreate::new(100)];

    let req = test::TestRequest::post()
        .uri("/games")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&games_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_games: Vec<Game> = test::read_body_json(resp).await;
    assert_eq!(created_games.len(), 1);
    assert!(created_games[0].team_id.is_none());
}
