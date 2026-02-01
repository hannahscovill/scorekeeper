//! Route handler for puzzle management (admin-only).

use actix_web::{get, put, web, HttpResponse};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::PuzzleDatabase;
use crate::middleware::auth::Claims;
use crate::models::error::AppError;

/// Request payload for setting a puzzle answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPuzzleRequest {
    /// The date of the puzzle in ISO format (YYYY-MM-DD).
    pub date: NaiveDate,
    /// The 5-letter answer word.
    pub word: String,
    /// Optional team ID for team-specific puzzles.
    #[serde(rename = "teamId")]
    pub team_id: Option<String>,
}

/// Response for setting a puzzle answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPuzzleResponse {
    /// The date of the puzzle.
    pub date: NaiveDate,
    /// The answer word.
    pub word: String,
    /// Optional team ID if set.
    #[serde(rename = "teamId", skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

/// Query parameters for getting puzzle answers.
#[derive(Debug, Clone, Deserialize)]
pub struct GetPuzzlesQuery {
    /// Start date for filtering puzzles (inclusive).
    pub start_date: Option<NaiveDate>,
    /// End date for filtering puzzles (inclusive).
    pub end_date: Option<NaiveDate>,
}

/// Response item for a puzzle answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuzzleAnswerResponse {
    /// The date of the puzzle.
    pub date: NaiveDate,
    /// The answer word.
    pub word: String,
}

/// Response for getting puzzle answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPuzzlesResponse {
    /// List of puzzle answers.
    pub puzzles: Vec<PuzzleAnswerResponse>,
}

/// GET /puzzle - Get puzzle answers (game admin only).
///
/// This endpoint allows game administrators to retrieve puzzle answers.
/// With no query parameters, returns all puzzles. Use start_date and end_date
/// to filter by date range. Only users with app_metadata.game_admin = true
/// can access this endpoint.
#[get("/puzzle")]
pub async fn get_puzzles(
    claims: Claims,
    query: web::Query<GetPuzzlesQuery>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    // Check if user is a game admin
    if !claims.is_game_admin() {
        return Err(AppError::Forbidden(
            "Only game administrators can view puzzle answers".to_string(),
        ));
    }

    let puzzles = puzzle_db
        .get_puzzle_answers(query.start_date, query.end_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let response = GetPuzzlesResponse {
        puzzles: puzzles
            .into_iter()
            .map(|p| PuzzleAnswerResponse {
                date: p.puzzle_date,
                word: p.word,
            })
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// PUT /puzzle - Set the answer for a puzzle (game admin only).
///
/// This endpoint allows game administrators to set or update the puzzle answer
/// for a specific date. Only users with app_metadata.game_admin = true
/// can access this endpoint.
#[put("/puzzle")]
pub async fn set_puzzle(
    claims: Claims,
    body: web::Json<SetPuzzleRequest>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    // Check if user is a game admin
    if !claims.is_game_admin() {
        return Err(AppError::Forbidden(
            "Only game administrators can set puzzle answers".to_string(),
        ));
    }

    let word = body.word.to_lowercase();

    // Validate word is exactly 5 lowercase letters
    if word.len() != 5 {
        return Err(AppError::bad_request("Word must be exactly 5 letters"));
    }

    if !word.chars().all(|c| c.is_ascii_lowercase()) {
        return Err(AppError::bad_request(
            "Word must contain only letters (a-z)",
        ));
    }

    // Set the puzzle answer
    puzzle_db
        .set_puzzle_answer(body.date, &word, body.team_id.as_deref())
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    Ok(HttpResponse::Ok().json(SetPuzzleResponse {
        date: body.date,
        word,
        team_id: body.team_id.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_puzzle_request_parsing() {
        let json = r#"{"date": "2026-02-15", "word": "crane"}"#;
        let req: SetPuzzleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.date, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
        assert_eq!(req.word, "crane");
        assert!(req.team_id.is_none());
    }

    #[test]
    fn test_set_puzzle_request_with_team_id() {
        let json = r#"{"date": "2026-02-15", "word": "crane", "teamId": "team-123"}"#;
        let req: SetPuzzleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.team_id, Some("team-123".to_string()));
    }

    #[test]
    fn test_set_puzzle_response_serialization() {
        let response = SetPuzzleResponse {
            date: NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
            word: "crane".to_string(),
            team_id: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("2026-02-15"));
        assert!(json.contains("crane"));
        assert!(!json.contains("teamId")); // should be skipped when None
    }

    #[test]
    fn test_set_puzzle_response_with_team_id() {
        let response = SetPuzzleResponse {
            date: NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
            word: "crane".to_string(),
            team_id: Some("team-123".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("teamId"));
        assert!(json.contains("team-123"));
    }
}
