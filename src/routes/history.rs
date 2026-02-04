//! Route handler for game history.

use actix_web::{get, web, HttpResponse};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

use crate::db::PuzzleDatabase;
use crate::middleware::auth::Claims;
use crate::models::error::AppError;
use crate::models::guess::LetterGrade;
use crate::services::grading::grade_guess;

/// Response for a single game in history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryGame {
    /// Unique identifier for this game (deterministic based on user_id and puzzle_date).
    pub game_id: Uuid,
    /// The puzzle date (YYYY-MM-DD).
    pub puzzle_date: NaiveDate,
    /// Number of guesses made (1-6).
    pub guesses_count: usize,
    /// Whether the player won.
    pub won: bool,
    /// Whether the game is still in progress.
    pub in_progress: bool,
    /// Graded guesses (grades only, no letters) for rendering mini game boards.
    /// Each inner Vec contains 5 LetterGrade values representing the result of each guess.
    pub graded_guesses: Vec<Vec<LetterGrade>>,
    /// When the game was started (ISO 8601).
    pub created_at: String,
    /// When the game was last updated (ISO 8601).
    pub updated_at: String,
}

/// Response for the history endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    /// The user ID.
    pub user_id: String,
    /// Total number of games.
    pub total_games: usize,
    /// Number of games won.
    pub games_won: usize,
    /// The list of games.
    pub games: Vec<HistoryGame>,
}

/// GET /history - Get the authenticated user's game history.
///
/// Returns all games played by the user, sorted by puzzle date (most recent first).
/// Includes graded guesses (grades only, no letters) for rendering mini game boards.
#[get("/history")]
#[instrument(name = "get_history", skip(puzzle_db), fields(user_id = %claims.sub))]
pub async fn get_history(
    claims: Claims,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    let user_id = claims.sub;

    let game_states = puzzle_db
        .get_user_game_states(&user_id)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let mut games = Vec::with_capacity(game_states.len());

    for state in &game_states {
        // Generate deterministic game_id based on user_id and puzzle_date
        let game_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("{}#{}", state.user_id, state.puzzle_date).as_bytes(),
        );

        // Fetch the puzzle answer to grade the guesses
        let graded_guesses = if let Some(answer) = puzzle_db
            .get_puzzle_answer(state.puzzle_date)
            .await
            .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
        {
            // Grade each guess and extract only the grades (no letters)
            state
                .guesses
                .iter()
                .map(|guess| {
                    grade_guess(guess, &answer)
                        .iter()
                        .map(|gl| gl.grade)
                        .collect()
                })
                .collect()
        } else {
            // No answer found - return empty grades
            Vec::new()
        };

        games.push(HistoryGame {
            game_id,
            puzzle_date: state.puzzle_date,
            guesses_count: state.guesses.len(),
            won: state.won,
            in_progress: state.is_in_progress(),
            graded_guesses,
            created_at: state.created_at.to_rfc3339(),
            updated_at: state.updated_at.to_rfc3339(),
        });
    }

    let games_won = games.iter().filter(|g| g.won).count();

    let response = HistoryResponse {
        user_id,
        total_games: games.len(),
        games_won,
        games,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_game_serialization() {
        let game = HistoryGame {
            game_id: Uuid::new_v4(),
            puzzle_date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            guesses_count: 3,
            won: true,
            in_progress: false,
            graded_guesses: vec![
                vec![
                    LetterGrade::Correct,
                    LetterGrade::Wrong,
                    LetterGrade::Contained,
                    LetterGrade::Wrong,
                    LetterGrade::Correct,
                ],
                vec![
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                ],
            ],
            created_at: "2026-01-15T10:00:00Z".to_string(),
            updated_at: "2026-01-15T10:05:00Z".to_string(),
        };

        let json = serde_json::to_string(&game).unwrap();
        assert!(json.contains("game_id"));
        assert!(json.contains("puzzle_date"));
        assert!(json.contains("guesses_count"));
        assert!(json.contains("won"));
        assert!(json.contains("in_progress"));
        assert!(json.contains("graded_guesses"));
        assert!(json.contains("correct"));
        assert!(json.contains("wrong"));
        assert!(json.contains("contained"));
    }

    #[test]
    fn test_history_response_serialization() {
        let response = HistoryResponse {
            user_id: "auth0|123".to_string(),
            total_games: 1,
            games_won: 1,
            games: vec![HistoryGame {
                game_id: Uuid::new_v4(),
                puzzle_date: NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                guesses_count: 3,
                won: true,
                in_progress: false,
                graded_guesses: vec![vec![
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                    LetterGrade::Correct,
                ]],
                created_at: "2026-01-15T10:00:00Z".to_string(),
                updated_at: "2026-01-15T10:05:00Z".to_string(),
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("user_id"));
        assert!(json.contains("total_games"));
        assert!(json.contains("games_won"));
        assert!(json.contains("games"));
        assert!(json.contains("graded_guesses"));
    }
}
