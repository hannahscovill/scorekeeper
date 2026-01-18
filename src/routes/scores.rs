//! Score-related route handlers.

use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use uuid::Uuid;

use crate::db::InMemoryDb;
use crate::middleware::auth::{extract_bearer_token_from_request, JwtAuth};
use crate::middleware::validation::extract_team_id_header;
use crate::models::AppError;

/// Placeholder endpoint for listing scores.
#[get("/scores")]
pub async fn list_scores() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "scores": [] }))
}

/// GET /scores/{game_id} - Retrieve scores for a specific game.
///
/// Path parameters:
/// - game_id: UUID of the game
///
/// Headers:
/// - Authorization: Bearer token (required)
/// - team-id: Optional UUID to filter scores by team
///
/// Responses:
/// - 200: ScoreList (array of scores)
/// - 400: Bad request (invalid game_id or team-id format)
/// - 401: Unauthorized (missing or invalid token)
#[get("/scores/{game_id}")]
pub async fn get_scores(
    req: HttpRequest,
    path: web::Path<String>,
    db: web::Data<InMemoryDb>,
    jwt_auth: web::Data<JwtAuth>,
) -> Result<HttpResponse, AppError> {
    // Validate JWT token
    let token = extract_bearer_token_from_request(&req).ok_or_else(AppError::unauthorized)?;

    jwt_auth
        .validate_token(&token)
        .map_err(|_| AppError::unauthorized())?;

    // Extract and validate game_id from path
    let game_id_str = path.into_inner();
    let game_id = Uuid::parse_str(&game_id_str)
        .map_err(|_| AppError::bad_request("Invalid game_id format: not a valid UUID"))?;

    // Extract optional team_id from header
    let team_id = extract_team_id_header(&req)?;

    // Query scores from database
    let scores = db
        .get_scores_by_game(game_id, team_id)
        .map_err(AppError::internal)?;

    Ok(HttpResponse::Ok().json(scores))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Score;
    use actix_web::{test, App};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_JWT_SECRET: &str = "test-secret-key-for-jwt-testing";

    fn create_test_token(user_id: Uuid, team_id: Option<Uuid>) -> String {
        use crate::middleware::auth::Claims;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        let claims = Claims {
            sub: user_id,
            exp: now + 3600,
            iat: now,
            team_id,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
        )
        .unwrap()
    }

    #[actix_web::test]
    async fn test_list_scores() {
        let app = test::init_service(App::new().service(list_scores)).await;
        let req = test::TestRequest::get().uri("/scores").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_get_scores_valid_request() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let score = Score::new(user_id, game_id, 100);
        db.insert_score(score).unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .service(get_scores),
        )
        .await;

        let token = create_test_token(user_id, None);
        let req = test::TestRequest::get()
            .uri(&format!("/scores/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, 100);
    }

    #[actix_web::test]
    async fn test_get_scores_invalid_game_id() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .service(get_scores),
        )
        .await;

        let token = create_test_token(Uuid::new_v4(), None);
        let req = test::TestRequest::get()
            .uri("/scores/not-a-valid-uuid")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_get_scores_with_team_id_filter() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let other_team_id = Uuid::new_v4();

        db.insert_score(Score::with_team(user_id, game_id, team_id, 100))
            .unwrap();
        db.insert_score(Score::with_team(user_id, game_id, other_team_id, 200))
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .service(get_scores),
        )
        .await;

        let token = create_test_token(user_id, None);
        let req = test::TestRequest::get()
            .uri(&format!("/scores/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .insert_header(("team-id", team_id.to_string()))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].score, 100);
        assert_eq!(body[0].team_id, Some(team_id));
    }

    #[actix_web::test]
    async fn test_get_scores_unauthorized_no_token() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .service(get_scores),
        )
        .await;

        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/scores/{}", game_id))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_get_scores_empty_result() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .service(get_scores),
        )
        .await;

        let token = create_test_token(Uuid::new_v4(), None);
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/scores/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn test_get_scores_invalid_team_id_header() {
        let db = InMemoryDb::new();
        let jwt_auth = JwtAuth::new(TEST_JWT_SECRET.to_string());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(db))
                .app_data(web::Data::new(jwt_auth))
                .service(get_scores),
        )
        .await;

        let token = create_test_token(Uuid::new_v4(), None);
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/scores/{}", game_id))
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .insert_header(("team-id", "not-a-valid-uuid"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }
}
