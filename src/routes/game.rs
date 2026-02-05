//! Route handler for loading game progress.

use actix_web::{get, web, HttpRequest, HttpResponse};
use chrono::NaiveDate;
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

use crate::db::PuzzleDatabase;
use crate::middleware::auth::{extract_bearer_token, extract_cookie_token, JwtAuth};
use crate::models::error::AppError;
use crate::models::guess::{GameState, GradedGame};
use crate::services::grade_guess;

/// GET /game/{puzzle_date} - Load game progress for a specific puzzle date.
///
/// Path parameters:
/// - puzzle_date: ISO date string (YYYY-MM-DD)
///
/// Authentication:
/// - Authorization header: Bearer <token> (validated as JWT)
/// - Cookie: wordle_session=<user_id> (used directly, for embedded games)
///
/// Responses:
/// - 200: GradedGame with current progress (may be empty if no guesses yet)
/// - 400: Bad request (invalid date format)
/// - 401: Unauthorized (missing or invalid authentication)
/// - 404: No puzzle found for this date
#[get("/game/{puzzle_date}")]
#[instrument(
    name = "get_game",
    skip(req, puzzle_db, jwt_auth),
    fields(puzzle_date = tracing::field::Empty, user_id = tracing::field::Empty)
)]
pub async fn get_game(
    req: HttpRequest,
    path: web::Path<String>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
    jwt_auth: web::Data<JwtAuth>,
) -> Result<HttpResponse, AppError> {
    // Try JWT first (Authorization header), fall back to cookie
    let user_id: String = if let Some(token) = extract_bearer_token(&req) {
        let claims = jwt_auth.validate_token(&token).await?;
        claims.sub
    } else if let Some(cookie_value) = extract_cookie_token(&req) {
        cookie_value
    } else {
        return Err(AppError::Unauthorized("Missing authentication".to_string()));
    };

    // Record user_id in span
    tracing::Span::current().record("user_id", &user_id);

    // Parse puzzle date from path
    let puzzle_date_str = path.into_inner();
    let puzzle_date = NaiveDate::parse_from_str(&puzzle_date_str, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Invalid date format. Expected YYYY-MM-DD"))?;

    // Record puzzle_date in span
    tracing::Span::current().record("puzzle_date", puzzle_date.to_string().as_str());

    // Verify the puzzle exists for this date
    let answer = puzzle_db
        .get_puzzle_answer(puzzle_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
        .ok_or_else(|| AppError::not_found("No puzzle found for this date"))?;

    // Get game state (or create empty one if no progress yet)
    let game_state = puzzle_db
        .get_game_state(&user_id, puzzle_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
        .unwrap_or_else(|| GameState::new(&user_id, puzzle_date));

    // Grade all guesses
    let moves: Vec<_> = game_state
        .guesses
        .iter()
        .map(|g| grade_guess(g, &answer))
        .collect();

    // Generate deterministic game ID
    let game_id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("{}#{}", user_id, puzzle_date).as_bytes(),
    );

    let response = GradedGame::new(game_id, &user_id, moves, game_state.won);

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_id_deterministic() {
        let user_id = "auth0|123";
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let game_id1 = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("{}#{}", user_id, date).as_bytes(),
        );
        let game_id2 = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("{}#{}", user_id, date).as_bytes(),
        );

        assert_eq!(game_id1, game_id2);
    }

    #[test]
    fn test_date_parsing() {
        let valid = NaiveDate::parse_from_str("2026-01-15", "%Y-%m-%d");
        assert!(valid.is_ok());
        assert_eq!(
            valid.unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()
        );

        let invalid = NaiveDate::parse_from_str("01-15-2026", "%Y-%m-%d");
        assert!(invalid.is_err());
    }
}
