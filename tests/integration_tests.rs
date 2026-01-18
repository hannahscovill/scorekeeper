//! Integration tests for the Scorekeeper API
//!
//! These tests verify full API request/response cycles, including:
//! - Score submission and retrieval
//! - Team filtering across endpoints
//! - Authentication scenarios
//! - Validation error responses

use actix_web::{http::StatusCode, test, web, App};
use jsonwebtoken::{encode, EncodingKey, Header};
use scorekeeper::db::InMemoryDb;
use scorekeeper::middleware::auth::{Claims, JwtAuth};
use scorekeeper::models::error::ErrorResponse;
use scorekeeper::models::score::{Score, ScoreCreate};
use scorekeeper::routes::{create_scores, get_scores, health_check};
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

/// Create shared database and JWT auth for tests.
fn create_test_data() -> (web::Data<InMemoryDb>, web::Data<JwtAuth>) {
    let db = web::Data::new(InMemoryDb::new());
    let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));
    (db, jwt_auth)
}

// ==================== Full Score Submission and Retrieval Cycle ====================

#[actix_web::test]
async fn test_full_score_submission_and_retrieval_cycle() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(health_check)
            .service(get_scores)
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, Some(team_id));

    // Step 1: POST scores
    let scores_to_create = vec![
        ScoreCreate::with_game(100, game_id),
        ScoreCreate::with_game(200, game_id),
        ScoreCreate::with_game(300, game_id),
    ];

    let post_req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&scores_to_create)
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::CREATED);

    let created_scores: Vec<Score> = test::read_body_json(post_resp).await;
    assert_eq!(created_scores.len(), 3);

    // Verify created scores have correct values
    assert_eq!(created_scores[0].score, 100);
    assert_eq!(created_scores[1].score, 200);
    assert_eq!(created_scores[2].score, 300);

    // Verify all scores belong to the same user and team
    for score in &created_scores {
        assert_eq!(score.user_id, user_id);
        assert_eq!(score.team_id, Some(team_id));
        assert_eq!(score.game_id, game_id);
    }

    // Step 2: GET scores for the game
    let get_req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);

    let retrieved_scores: Vec<Score> = test::read_body_json(get_resp).await;
    assert_eq!(retrieved_scores.len(), 3);

    // Verify we can find all the created scores
    let score_values: Vec<i32> = retrieved_scores.iter().map(|s| s.score).collect();
    assert!(score_values.contains(&100));
    assert!(score_values.contains(&200));
    assert!(score_values.contains(&300));
}

#[actix_web::test]
async fn test_score_submission_generates_unique_ids() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    let scores_to_create = vec![ScoreCreate::new(100), ScoreCreate::new(200)];

    let req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&scores_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_scores: Vec<Score> = test::read_body_json(resp).await;
    assert_eq!(created_scores.len(), 2);

    // Each score should have a unique ID
    assert_ne!(created_scores[0].id, created_scores[1].id);
    // Each score without game_id should get a unique game_id
    assert_ne!(created_scores[0].game_id, created_scores[1].game_id);
}

// ==================== Team Filtering Across Endpoints ====================

#[actix_web::test]
async fn test_team_filtering_across_endpoints() {
    let (db, jwt_auth) = create_test_data();

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let team_a = Uuid::new_v4();
    let team_b = Uuid::new_v4();

    // Pre-populate database with scores for different teams
    db.insert_score(Score::with_team(user_id, game_id, team_a, 100))
        .unwrap();
    db.insert_score(Score::with_team(user_id, game_id, team_a, 150))
        .unwrap();
    db.insert_score(Score::with_team(user_id, game_id, team_b, 200))
        .unwrap();
    db.insert_score(Score::with_team(user_id, game_id, team_b, 250))
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Get scores filtered by team A
    let req_team_a = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", team_a.to_string()))
        .to_request();

    let resp_team_a = test::call_service(&app, req_team_a).await;
    assert_eq!(resp_team_a.status(), StatusCode::OK);

    let scores_team_a: Vec<Score> = test::read_body_json(resp_team_a).await;
    assert_eq!(scores_team_a.len(), 2);
    for score in &scores_team_a {
        assert_eq!(score.team_id, Some(team_a));
    }

    // Get scores filtered by team B
    let req_team_b = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", team_b.to_string()))
        .to_request();

    let resp_team_b = test::call_service(&app, req_team_b).await;
    assert_eq!(resp_team_b.status(), StatusCode::OK);

    let scores_team_b: Vec<Score> = test::read_body_json(resp_team_b).await;
    assert_eq!(scores_team_b.len(), 2);
    for score in &scores_team_b {
        assert_eq!(score.team_id, Some(team_b));
    }

    // Get all scores (no team filter)
    let req_all = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp_all = test::call_service(&app, req_all).await;
    assert_eq!(resp_all.status(), StatusCode::OK);

    let all_scores: Vec<Score> = test::read_body_json(resp_all).await;
    assert_eq!(all_scores.len(), 4);
}

#[actix_web::test]
async fn test_team_filtering_with_nonexistent_team() {
    let (db, jwt_auth) = create_test_data();

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let nonexistent_team = Uuid::new_v4();

    // Insert some scores
    db.insert_score(Score::with_team(user_id, game_id, team_id, 100))
        .unwrap();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Filter by a team that has no scores
    let req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .insert_header(("team-id", nonexistent_team.to_string()))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let scores: Vec<Score> = test::read_body_json(resp).await;
    assert!(scores.is_empty());
}

// ==================== Unauthorized Access Tests ====================

#[actix_web::test]
async fn test_unauthorized_access_both_endpoints() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores)
            .service(create_scores),
    )
    .await;

    let game_id = Uuid::new_v4();

    // Test GET without token
    let get_req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::UNAUTHORIZED);

    let get_body: ErrorResponse = test::read_body_json(get_resp).await;
    assert_eq!(get_body.error.code, "UNAUTHORIZED");

    // Test POST without token
    let post_req = test::TestRequest::post()
        .uri("/scores")
        .set_json(&vec![ScoreCreate::new(100)])
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::UNAUTHORIZED);

    let post_body: ErrorResponse = test::read_body_json(post_resp).await;
    assert_eq!(post_body.error.code, "UNAUTHORIZED");
}

#[actix_web::test]
async fn test_unauthorized_with_invalid_token() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores)
            .service(create_scores),
    )
    .await;

    let game_id = Uuid::new_v4();

    // Test GET with invalid token
    let get_req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", "Bearer invalid-token-here"))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::UNAUTHORIZED);

    // Test POST with invalid token
    let post_req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", "Bearer invalid-token-here"))
        .set_json(&vec![ScoreCreate::new(100)])
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_unauthorized_with_expired_token() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores)
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let expired_token = generate_expired_token(TEST_JWT_SECRET, user_id);

    // Test GET with expired token
    let get_req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", expired_token)))
        .to_request();

    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::UNAUTHORIZED);

    // Test POST with expired token
    let post_req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", expired_token)))
        .set_json(&vec![ScoreCreate::new(100)])
        .to_request();

    let post_resp = test::call_service(&app, post_req).await;
    assert_eq!(post_resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn test_unauthorized_with_wrong_secret() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    // Token signed with different secret
    let wrong_secret_token = generate_test_token("wrong-secret", user_id, None);

    let req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", wrong_secret_token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ==================== Validation Error Response Tests ====================

#[actix_web::test]
async fn test_validation_error_responses() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test 422 - Empty score list
    let empty_scores: Vec<ScoreCreate> = vec![];
    let req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&empty_scores)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: ErrorResponse = test::read_body_json(resp).await;
    assert_eq!(body.error.code, "VALIDATION_ERROR");
    assert!(body.error.details.is_some());
    let details = body.error.details.unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].field, "scores");
}

#[actix_web::test]
async fn test_bad_request_invalid_game_id() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test 400 - Invalid game_id format
    let req = test::TestRequest::get()
        .uri("/scores/not-a-valid-uuid")
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
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test 400 - Invalid team-id header format
    let req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
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
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
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
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
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
async fn test_get_scores_empty_game() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Get scores for a game with no scores
    let req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let scores: Vec<Score> = test::read_body_json(resp).await;
    assert!(scores.is_empty());
}

#[actix_web::test]
async fn test_score_values_preserved_correctly() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let game_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    // Test various score values including negative and zero
    let scores_to_create = vec![
        ScoreCreate::with_game(-100, game_id),
        ScoreCreate::with_game(0, game_id),
        ScoreCreate::with_game(999999, game_id),
    ];

    let req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&scores_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_scores: Vec<Score> = test::read_body_json(resp).await;
    let score_values: Vec<i32> = created_scores.iter().map(|s| s.score).collect();

    assert!(score_values.contains(&-100));
    assert!(score_values.contains(&0));
    assert!(score_values.contains(&999999));
}

#[actix_web::test]
async fn test_multiple_users_same_game() {
    let (db, jwt_auth) = create_test_data();

    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let game_id = Uuid::new_v4();

    // Insert scores from different users
    db.insert_score(Score::new(user_a, game_id, 100)).unwrap();
    db.insert_score(Score::new(user_b, game_id, 200)).unwrap();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(get_scores),
    )
    .await;

    let token = generate_test_token(TEST_JWT_SECRET, user_a, None);

    // Get all scores for the game
    let req = test::TestRequest::get()
        .uri(&format!("/scores/{}", game_id))
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let scores: Vec<Score> = test::read_body_json(resp).await;
    assert_eq!(scores.len(), 2);

    // Verify both users' scores are returned
    let user_ids: Vec<Uuid> = scores.iter().map(|s| s.user_id).collect();
    assert!(user_ids.contains(&user_a));
    assert!(user_ids.contains(&user_b));
}

#[actix_web::test]
async fn test_jwt_claims_user_id_used_for_created_scores() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let token = generate_test_token(TEST_JWT_SECRET, user_id, Some(team_id));

    let scores_to_create = vec![ScoreCreate::new(100)];

    let req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&scores_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_scores: Vec<Score> = test::read_body_json(resp).await;
    assert_eq!(created_scores.len(), 1);

    // The user_id and team_id should come from the JWT claims
    assert_eq!(created_scores[0].user_id, user_id);
    assert_eq!(created_scores[0].team_id, Some(team_id));
}

#[actix_web::test]
async fn test_jwt_without_team_id_creates_scores_without_team() {
    let (db, jwt_auth) = create_test_data();

    let app = test::init_service(
        App::new()
            .app_data(db.clone())
            .app_data(jwt_auth.clone())
            .service(create_scores),
    )
    .await;

    let user_id = Uuid::new_v4();
    // Token without team_id
    let token = generate_test_token(TEST_JWT_SECRET, user_id, None);

    let scores_to_create = vec![ScoreCreate::new(100)];

    let req = test::TestRequest::post()
        .uri("/scores")
        .insert_header(("Authorization", format!("Bearer {}", token)))
        .set_json(&scores_to_create)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let created_scores: Vec<Score> = test::read_body_json(resp).await;
    assert_eq!(created_scores.len(), 1);
    assert!(created_scores[0].team_id.is_none());
}
