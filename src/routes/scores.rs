//! Score-related route handlers.

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use uuid::Uuid;

use crate::db::InMemoryDb;
use crate::middleware::auth::{extract_bearer_token_from_request, Claims, JwtAuth};
use crate::middleware::validation::validate_score_create_list;
use crate::models::error::AppError;
use crate::models::score::{Score, ScoreCreateList, ScoreList};

/// Placeholder endpoint for listing scores.
#[get("/scores")]
pub async fn list_scores() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "scores": [] }))
}

/// Create scores endpoint for batch score submission.
#[post("/scores")]
pub async fn create_scores(
    req: HttpRequest,
    body: web::Json<ScoreCreateList>,
    db: web::Data<InMemoryDb>,
    jwt_auth: web::Data<JwtAuth>,
) -> Result<HttpResponse, AppError> {
    // Extract and validate JWT token
    let token = extract_bearer_token_from_request(&req)
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

    let claims: Claims = jwt_auth
        .validate_token(&token)
        .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

    // Validate the score list
    let scores_input = body.into_inner();
    validate_score_create_list(&scores_input)?;

    // Create Score objects
    let mut created_scores: ScoreList = Vec::new();
    for score_create in scores_input {
        let score = Score {
            id: Uuid::new_v4(),
            user_id: claims.sub,
            game_id: score_create.game_id.unwrap_or_else(Uuid::new_v4),
            team_id: claims.team_id,
            score: score_create.score,
            created_at: Utc::now(),
        };
        created_scores.push(score.clone());
        db.insert_score(score).map_err(AppError::InternalError)?;
    }

    Ok(HttpResponse::Created().json(created_scores))
}

/// Configure scores routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_scores).service(create_scores);
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use actix_web::{test, App};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use crate::models::score::ScoreCreate;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_JWT_SECRET: &str = "test-secret-key-for-jwt-testing";

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

    #[actix_web::test]
    async fn test_list_scores() {
        let app = test::init_service(App::new().service(list_scores)).await;
        let req = test::TestRequest::get().uri("/scores").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_create_scores_success() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let scores = vec![ScoreCreate::new(100), ScoreCreate::new(200)];

        let req = test::TestRequest::post()
            .uri("/scores")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 2);
        assert_eq!(body[0].score, 100);
        assert_eq!(body[1].score, 200);
        assert_eq!(body[0].user_id, claims.sub);
        assert_eq!(body[1].user_id, claims.sub);
    }

    #[actix_web::test]
    async fn test_create_scores_empty_array_returns_422() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let scores: Vec<ScoreCreate> = vec![];

        let req = test::TestRequest::post()
            .uri("/scores")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[actix_web::test]
    async fn test_create_scores_unauthorized_no_token() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
        )
        .await;

        let scores = vec![ScoreCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/scores")
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_create_scores_unauthorized_invalid_token() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
        )
        .await;

        let scores = vec![ScoreCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/scores")
            .insert_header(("Authorization", "Bearer invalid-token"))
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn test_create_scores_batch_multiple() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
        )
        .await;

        let claims = create_test_claims();
        let token = create_test_token(&claims);

        let game_id = Uuid::new_v4();
        let scores = vec![
            ScoreCreate::with_game(100, game_id),
            ScoreCreate::with_game(200, game_id),
            ScoreCreate::with_game(300, game_id),
        ];

        let req = test::TestRequest::post()
            .uri("/scores")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 3);

        // All should have the same game_id
        for score in &body {
            assert_eq!(score.game_id, game_id);
        }
    }

    #[actix_web::test]
    async fn test_create_scores_user_id_from_jwt() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
        )
        .await;

        let claims = create_test_claims();
        let expected_user_id = claims.sub;
        let expected_team_id = claims.team_id;
        let token = create_test_token(&claims);

        let scores = vec![ScoreCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/scores")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].user_id, expected_user_id);
        assert_eq!(body[0].team_id, expected_team_id);
    }

    #[actix_web::test]
    async fn test_create_scores_no_team_id_in_jwt() {
        let db = web::Data::new(InMemoryDb::new());
        let jwt_auth = web::Data::new(JwtAuth::new(TEST_JWT_SECRET.to_string()));

        let app = test::init_service(
            App::new()
                .app_data(db.clone())
                .app_data(jwt_auth.clone())
                .service(create_scores),
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

        let scores = vec![ScoreCreate::new(100)];

        let req = test::TestRequest::post()
            .uri("/scores")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .set_json(&scores)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Vec<Score> = test::read_body_json(resp).await;
        assert_eq!(body.len(), 1);
        assert!(body[0].team_id.is_none());
    }
}
