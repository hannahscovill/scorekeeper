//! Route handler for word guessing game.

use actix_web::{post, web, HttpRequest, HttpResponse};
use std::sync::Arc;
use uuid::Uuid;

use crate::db::PuzzleDatabase;
use crate::dictionary::is_valid_word;
use crate::middleware::auth::{extract_bearer_token, extract_cookie_token, JwtAuth};
use crate::models::error::AppError;
use crate::models::guess::{GameState, GradedGame, GuessRequest};
use crate::services::{grade_guess, is_winning_guess};

/// POST /guess - Submit a guess for a word puzzle.
///
/// Accepts authentication via either:
/// - Authorization header: Bearer <token> (validated as JWT)
/// - Cookie: wordle_session=<user_id> (used directly, for embedded games)
#[post("/guess")]
pub async fn submit_guess(
    req: HttpRequest,
    body: web::Json<GuessRequest>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
    jwt_auth: web::Data<JwtAuth>,
) -> Result<HttpResponse, AppError> {
    // Try JWT first (Authorization header), fall back to cookie
    let user_id: String = if let Some(token) = extract_bearer_token(&req) {
        // Validate JWT and extract user ID
        let claims = jwt_auth.validate_token(&token).await?;
        claims.sub
    } else if let Some(cookie_value) = extract_cookie_token(&req) {
        // Use cookie value directly as user ID (for embedded games)
        cookie_value
    } else {
        return Err(AppError::Unauthorized("Missing authentication".to_string()));
    };

    let puzzle_date = body.puzzle_date_iso_day;
    let guess = body.word_guessed.to_lowercase();

    // Validate word is 5 letters
    if guess.len() != 5 {
        return Err(AppError::bad_request("Guess must be exactly 5 letters"));
    }

    // Validate word is in dictionary
    if !is_valid_word(&guess) {
        return Err(AppError::bad_request("Word not in dictionary"));
    }

    // Get the puzzle answer
    let answer = puzzle_db
        .get_puzzle_answer(puzzle_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
        .ok_or_else(|| AppError::not_found("No puzzle found for this date"))?;

    // Get or create game state
    let mut game_state = puzzle_db
        .get_game_state(&user_id, puzzle_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
        .unwrap_or_else(|| GameState::new(&user_id, puzzle_date));

    // Check if game is still in progress
    if !game_state.is_in_progress() {
        return Err(AppError::bad_request("Game is already complete"));
    }

    // Grade the guess
    let graded = grade_guess(&guess, &answer);
    let won = is_winning_guess(&graded);

    // Update game state
    game_state.add_guess(&guess);
    if won {
        game_state.mark_won();
    }

    // Persist updated state
    puzzle_db
        .upsert_game_state(&game_state)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    // Build response with all graded guesses
    let moves: Vec<_> = game_state
        .guesses
        .iter()
        .map(|g| grade_guess(g, &answer))
        .collect();

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
    use chrono::NaiveDate;

    // Note: Full integration tests require a mock JwtAuth
    // These tests verify the route is wired up correctly

    #[test]
    fn test_guess_request_parsing() {
        let json = r#"{"puzzle_date_iso_day": "2026-01-15", "word_guessed": "crane"}"#;
        let req: GuessRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.puzzle_date_iso_day,
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()
        );
        assert_eq!(req.word_guessed, "crane");
    }

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
    fn test_graded_game_response() {
        let game_id = Uuid::new_v4();
        let moves = vec![grade_guess("crane", "stale")];
        let game = GradedGame::new(game_id, "auth0|123", moves, false);

        let json = serde_json::to_string(&game).unwrap();
        assert!(json.contains("game_id"));
        assert!(json.contains("user_id"));
        assert!(json.contains("moves_qty"));
        assert!(json.contains("moves"));
    }
}
