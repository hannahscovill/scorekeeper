//! Game-related route handlers.

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use uuid::Uuid;

use crate::config::Config;
use crate::db::InMemoryDb;
use crate::middleware::auth::{dev_claims, extract_bearer_token_from_request, Claims, JwtAuth};
use crate::middleware::validation::{extract_team_id_header, validate_game_create_list};
use crate::models::error::AppError;
use crate::models::game::{Game, GameCreateList, GameList};

/// Placeholder endpoint for listing games.
#[get("/games")]
pub async fn list_games() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "games": [] }))
}

/// Create games endpoint for batch game submission.
#[post("/games")]
pub async fn create_games(
    req: HttpRequest,
    body: web::Json<GameCreateList>,
    db: web::Data<InMemoryDb>,
    jwt_auth: web::Data<JwtAuth>,
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    // Extract and validate JWT token (with optional bypass for local development)
    let claims: Claims = if config.bypass_auth() {
        tracing::warn!("AUTH BYPASS: Skipping JWT validation for POST /games");
        dev_claims()
    } else {
        let token = extract_bearer_token_from_request(&req)
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;
        jwt_auth
            .validate_token(&token)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?
    };

    // Validate the game list
    let games_input = body.into_inner();
    validate_game_create_list(&games_input)?;

    // Create Game objects
    let mut created_games: GameList = Vec::new();
    for game_create in games_input {
        let game = Game {
            id: Uuid::new_v4(),
            user_id: claims.sub,
            game_id: game_create.game_id.unwrap_or_else(Uuid::new_v4),
            team_id: claims.team_id,
            score: game_create.score,
            created_at: Utc::now(),
        };
        created_games.push(game.clone());
        db.insert_game(game).map_err(AppError::InternalError)?;
    }

    Ok(HttpResponse::Created().json(created_games))
}

/// GET /games/{game_id} - Retrieve games for a specific game session.
///
/// Path parameters:
/// - game_id: UUID of the game session
///
/// Headers:
/// - Authorization: Bearer token (required, unless BYPASS_AUTH=true)
/// - team-id: Optional UUID to filter games by team
///
/// Responses:
/// - 200: GameList (array of games)
/// - 400: Bad request (invalid game_id or team-id format)
/// - 401: Unauthorized (missing or invalid token)
#[get("/games/{game_id}")]
pub async fn get_games(
    req: HttpRequest,
    path: web::Path<String>,
    db: web::Data<InMemoryDb>,
    jwt_auth: web::Data<JwtAuth>,
    config: web::Data<Config>,
) -> Result<HttpResponse, AppError> {
    // Validate JWT token (with optional bypass for local development)
    if config.bypass_auth() {
        tracing::warn!("AUTH BYPASS: Skipping JWT validation for GET /games/{{game_id}}");
    } else {
        let token = extract_bearer_token_from_request(&req).ok_or_else(AppError::unauthorized)?;
        jwt_auth
            .validate_token(&token)
            .map_err(|_| AppError::unauthorized())?;
    }

    // Extract and validate game_id from path
    let game_id_str = path.into_inner();
    let game_id = Uuid::parse_str(&game_id_str)
        .map_err(|_| AppError::bad_request("Invalid game_id format: not a valid UUID"))?;

    // Extract optional team_id from header
    let team_id = extract_team_id_header(&req)?;

    // Query games from database
    let games = db
        .get_games_by_game_id(game_id, team_id)
        .map_err(AppError::internal)?;

    Ok(HttpResponse::Ok().json(games))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::game::GameCreate;
    use actix_web::http::StatusCode;
    use actix_web::{test, App};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_JWT_SECRET: &str = "test-secret-key-for-jwt-testing";

    fn create_test_config() -> Config {
        Config {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database_url: None,
            jwt_secret: TEST_JWT_SECRET.to_string(),
            bypass_auth: false, // Tests should validate auth properly
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }

    fn get_current_timestamp() -> usize {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
    }

    fn create_test_token(claims: &Claims) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
        )
        .unwrap()
    }

    fn create_test_claims() -> Claims {
        let now = get_current_timestamp();
        Claims {
            sub: Uuid::new_v4(),
            exp: now + 3600,
            iat: now,
            team_id: Some(Uuid::new_v4()),
        }
    }

    fn create_test_claims_with_user(user_id: Uuid, team_id: Option<Uuid>) -> Claims {
        let now = get_current_timestamp();
        Claims {
            sub: user_id,
            exp: now + 3600,
            iat: now,
            team_id,
        }
    }

    fn create_expired_claims() -> Claims {
        let now = get_current_timestamp();
        Claims {
            sub: Uuid::new_v4(),
            exp: now - 3600, // expired 1 hour ago
            iat: now - 7200,
            team_id: None,
        }
    }

    // ==================== list_games tests ====================

    #[actix_web::test]
    async fn test_list_games() {
        let app = test::init_service(App::new().service(list_games)).await;
        let req = test::TestRequest::get().uri("/games").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    // ==================== GET /games/{game_id} tests ====================

    #[actix_web::test]
    async fn test_get_games_valid_request() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let game = Game::new(user_id, game_id, 100);
        db.insert_game(game).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_test_claims_with_user(user_id, None);
        let token = create_test_token(&claims);
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, 100);
    }

    #[actix_web::test]
    async fn test_get_games_invalid_game_id() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);
        let req = test::TestRequest::get()
            .uri("/games/not-a-valid-uuid")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_get_games_with_team_id_filter() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let other_team_id = Uuid::new_v4();

        db.insert_game(Game::with_team(user_id, game_id, team_id, 100))
            .unwrap();
        db.insert_game(Game::with_team(user_id, game_id, other_team_id, 200))
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_test_claims_with_user(user_id, None);
        let token = create_test_token(&claims);
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .insert_header(("team-id", team_id.to_string()))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, 100);
        assert_eq!(body[0].team_id, Some(team_id));
    }

    #[actix_web::test]
    async fn test_get_games_unauthorized_no_token() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_get_games_unauthorized_invalid_token() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", "Bearer invalid-token"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_get_games_unauthorized_expired_token() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_expired_claims();
        let token = create_test_token(&claims);
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_get_games_empty_result() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn test_get_games_invalid_team_id_header() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .insert_header(("team-id", "not-a-valid-uuid"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_get_games_multiple_games_for_session() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();

        // Insert multiple games for the same game session
        db.insert_game(Game::new(user_id, game_id, 100)).unwrap();
        db.insert_game(Game::new(user_id, game_id, 200)).unwrap();
        db.insert_game(Game::new(user_id, game_id, 300)).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .app_data(web::Data::new(create_test_config()))
                .service(get_games),
        )
        .await;

        let claims = create_test_claims_with_user(user_id, None);
        let token = create_test_token(&claims);
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 3);
    }

    // ==================== POST /games tests ====================

    #[actix_web::test]
    async fn test_create_games_success() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let games = vec![GameCreate::new(100), GameCreate::new(200)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 2);
        assert_eq!(body[0].score, 100);
        assert_eq!(body[1].score, 200);
        assert_eq!(body[0].user_id, claims.sub);
        assert_eq!(body[1].user_id, claims.sub);
    }

    #[actix_web::test]
    async fn test_create_games_empty_array_returns_422() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let games: Vec<GameCreate> = vec![];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[actix_web::test]
    async fn test_create_games_unauthorized_no_token() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let games = vec![GameCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/games")
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_create_games_unauthorized_invalid_token() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let games = vec![GameCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", "Bearer invalid-token"))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_create_games_unauthorized_expired_token() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_expired_claims();
        let token = create_test_token(&claims);

        let games = vec![GameCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_create_games_batch_multiple() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let game_id = Uuid::new_v4();
        let games = vec![
            GameCreate::with_game(100, game_id),
            GameCreate::with_game(200, game_id),
            GameCreate::with_game(300, game_id),
        ];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 3);

        // All should have the same game_id
        for game in &body {
            assert_eq!(game.game_id, game_id);
        }
    }

    #[actix_web::test]
    async fn test_create_games_user_id_from_jwt() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let expected_user_id = claims.sub;
        let expected_team_id = claims.team_id;
        let token = create_test_token(&claims);

        let games = vec![GameCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].user_id, expected_user_id);
        assert_eq!(body[0].team_id, expected_team_id);
    }

    #[actix_web::test]
    async fn test_create_games_no_team_id_in_jwt() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let now = get_current_timestamp();
        let claims = Claims {
            sub: Uuid::new_v4(),
            exp: now + 3600,
            iat: now,
            team_id: None,
        };
        let token = create_test_token(&claims);

        let games = vec![GameCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert!(body[0].team_id.is_none());
    }

    #[actix_web::test]
    async fn test_create_games_generates_game_id_when_not_provided() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        // Create games without game_id
        let games = vec![GameCreate::new(100), GameCreate::new(200)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 2);
        // Each should have a different generated game_id
        assert_ne!(body[0].game_id, body[1].game_id);
    }

    #[actix_web::test]
    async fn test_create_games_single_item() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let games = vec![GameCreate::new(42)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, 42);
    }

    #[actix_web::test]
    async fn test_create_games_negative_score_value() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        // Negative score values should be allowed (i32)
        let games = vec![GameCreate::new(-50)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, -50);
    }

    #[actix_web::test]
    async fn test_create_games_zero_score_value() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .app_data(web::Data::new(create_test_config()))
                .service(create_games),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let games = vec![GameCreate::new(0)];

        let req = test::TestRequest::post()
            .uri("/games")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&games)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Game> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, 0);
    }
}
