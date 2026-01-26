//! Game-related route handlers.

use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::db::GameDatabase;
use crate::middleware::auth::Claims;
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
    claims: Claims,
    body: web::Json<GameCreateList>,
    db: web::Data<Arc<dyn GameDatabase>>,
) -> Result<HttpResponse, AppError> {
    // Validate the game list
    let games_input = body.into_inner();
    validate_game_create_list(&games_input)?;

    // Create Game objects
    let mut created_games: GameList = Vec::new();
    for game_create in games_input {
        let game = Game {
            id: Uuid::new_v4(),
            user_id: claims.sub.clone(),
            game_id: game_create.game_id.unwrap_or_else(Uuid::new_v4),
            team_id: None, // Team ID can be added via request body if needed
            score: game_create.score,
            created_at: Utc::now(),
        };
        created_games.push(game.clone());
        db.insert_game(game)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
    }

    Ok(HttpResponse::Created().json(created_games))
}

/// GET /games/{game_id} - Retrieve games for a specific game session.
///
/// Path parameters:
/// - game_id: UUID of the game session
///
/// Headers:
/// - Authorization: Bearer token (required)
/// - team-id: Optional UUID to filter games by team
///
/// Responses:
/// - 200: GameList (array of games)
/// - 400: Bad request (invalid game_id or team-id format)
/// - 401: Unauthorized (missing or invalid token)
#[get("/games/{game_id}")]
pub async fn get_games(
    _claims: Claims, // Auth required but claims not used
    req: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Arc<dyn GameDatabase>>,
) -> Result<HttpResponse, AppError> {
    // Extract and validate game_id from path
    let game_id_str = path.into_inner();
    let game_id = Uuid::parse_str(&game_id_str)
        .map_err(|_| AppError::bad_request("Invalid game_id format: not a valid UUID"))?;

    // Extract optional team_id from header
    let team_id = extract_team_id_header(&req)?;

    // Query games from database
    let games = db
        .get_games_by_game_id(game_id, team_id)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(HttpResponse::Ok().json(games))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    // NOTE: Most tests were removed during Auth0 migration.
    // The handlers now use Auth0 RS256 JWTs which require JWKS validation.
    // Tests would need to either:
    // 1. Mock the JWKS endpoint
    // 2. Use a test-specific Claims injection mechanism
    // 3. Be converted to integration tests with real Auth0 tokens

    #[actix_web::test]
    async fn test_list_games() {
        let app = test::init_service(App::new().service(list_games)).await;
        let req = test::TestRequest::get().uri("/games").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }
}
