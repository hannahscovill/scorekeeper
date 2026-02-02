//! Route handler for puzzle management.
//!
//! GET endpoints are accessible to all authenticated users, but answers are only
//! visible to game administrators. PUT and cache clear endpoints are admin-only.

use actix_web::{get, post, put, web, HttpResponse};
use chrono::NaiveDate;
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

use crate::db::{clear_answer_cache, PuzzleDatabase};
use crate::dictionary;
use crate::dictionary::is_valid_word;
use crate::middleware::auth::Claims;
use crate::middleware::validation::{deserialize_optional_date, validate_date_range};
use crate::models::error::AppError;

/// Request payload for setting a puzzle answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPuzzleRequest {
    /// The date of the puzzle in ISO format (YYYY-MM-DD).
    pub date: NaiveDate,
    /// The 5-letter answer word. Required unless set_random_unused_word is true.
    pub word: Option<String>,
    /// Optional team ID for team-specific puzzles.
    #[serde(rename = "teamId")]
    pub team_id: Option<String>,
    /// If true, automatically select a random word that hasn't been used yet.
    #[serde(default)]
    pub set_random_unused_word: bool,
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
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub start_date: Option<NaiveDate>,
    /// End date for filtering puzzles (inclusive).
    #[serde(default, deserialize_with = "deserialize_optional_date")]
    pub end_date: Option<NaiveDate>,
    /// If true, omit the answer words from the response (only return dates).
    #[serde(default)]
    pub omit_answers: bool,
}

impl GetPuzzlesQuery {
    /// Validates that date range parameters are consistent.
    /// Both start_date and end_date must be provided together, or neither.
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_date_range(self.start_date, self.end_date)
    }
}

/// Response item for a puzzle answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuzzleAnswerResponse {
    /// The date of the puzzle.
    pub date: NaiveDate,
    /// The answer word. Omitted when omit_answers=true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
}

/// Response for getting puzzle answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPuzzlesResponse {
    /// List of puzzle answers.
    pub puzzles: Vec<PuzzleAnswerResponse>,
}

/// GET /puzzles/{date} - Get a single puzzle by date.
///
/// This endpoint allows authenticated users to check if a puzzle exists for a
/// specific date. Only game administrators (app_metadata.game_admin = true)
/// will see the answer word; non-admin users only see the date.
#[get("/puzzles/{date}")]
pub async fn get_puzzle_by_date(
    claims: Claims,
    path: web::Path<NaiveDate>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    let is_admin = claims.is_game_admin();
    let date = path.into_inner();

    let answer = puzzle_db
        .get_puzzle_answer(date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    match answer {
        Some(word) => Ok(HttpResponse::Ok().json(PuzzleAnswerResponse {
            date,
            // Only include the answer if user is an admin
            word: if is_admin { Some(word) } else { None },
        })),
        None => Err(AppError::not_found(format!(
            "No puzzle found for date {}",
            date
        ))),
    }
}

/// GET /puzzles - Get puzzles.
///
/// This endpoint allows authenticated users to retrieve puzzles.
/// With no query parameters, returns all puzzles. Use start_date and end_date
/// to filter by date range. Only game administrators (app_metadata.game_admin = true)
/// will see the answer words; non-admin users only see dates.
/// Admins can use omit_answers=true to only return dates without the answer words.
#[get("/puzzles")]
pub async fn get_puzzles(
    claims: Claims,
    query: web::Query<GetPuzzlesQuery>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    let is_admin = claims.is_game_admin();

    // Validate that both dates are provided if either is specified
    if let Err(msg) = query.validate() {
        return Err(AppError::bad_request(msg));
    }

    let puzzles = puzzle_db
        .get_puzzle_answers(query.start_date, query.end_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    // Non-admin users never see answers; admins see answers unless omit_answers=true
    let omit_answers = !is_admin || query.omit_answers;
    let response = GetPuzzlesResponse {
        puzzles: puzzles
            .into_iter()
            .map(|p| PuzzleAnswerResponse {
                date: p.puzzle_date,
                word: if omit_answers { None } else { Some(p.word) },
            })
            .collect(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// PUT /puzzles - Set the answer for a puzzle (game admin only).
///
/// This endpoint allows game administrators to set or update the puzzle answer
/// for a specific date. Only users with app_metadata.game_admin = true
/// can access this endpoint.
///
/// You can either provide a specific word, or set `setRandomUnusedWord: true`
/// to automatically select a random word that hasn't been used in any puzzle yet.
#[put("/puzzles")]
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

    let word = if body.set_random_unused_word {
        // Get all existing puzzle answers to find used words
        let existing_puzzles = puzzle_db
            .get_puzzle_answers(None, None)
            .await
            .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

        let used_words: HashSet<String> = existing_puzzles
            .into_iter()
            .map(|p| p.word.to_lowercase())
            .collect();

        // Find a random unused word from the dictionary
        let mut rng = rand::thread_rng();
        let unused_word = dictionary::all_words()
            .filter(|word| !used_words.contains(*word))
            .choose(&mut rng);

        match unused_word {
            Some(word) => word.to_string(),
            None => {
                return Err(AppError::bad_request(
                    "No unused words available in the dictionary",
                ))
            }
        }
    } else {
        // Use the provided word
        match &body.word {
            Some(w) => w.to_lowercase(),
            None => {
                return Err(AppError::bad_request(
                    "Either provide a word or set set_random_unused_word to true",
                ))
            }
        }
    };

    // Validate word is exactly 5 lowercase letters
    if word.len() != 5 {
        return Err(AppError::bad_request("Word must be exactly 5 letters"));
    }

    if !word.chars().all(|c| c.is_ascii_lowercase()) {
        return Err(AppError::bad_request(
            "Word must contain only letters (a-z)",
        ));
    }

    // Validate word is in dictionary
    if !is_valid_word(&word) {
        return Err(AppError::bad_request("Word not in dictionary"));
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

/// POST /puzzles/cache/clear - Clear the puzzle answer cache (game admin only).
///
/// This endpoint allows game administrators to clear the in-memory cache of puzzle
/// answers. This is useful when puzzle answers have been modified directly in the
/// database (e.g., via AWS console) and the server needs to pick up the changes.
///
/// Note: Each server instance has its own cache, so in a load-balanced environment
/// this endpoint should be called on each instance, or all instances should be restarted.
#[post("/puzzles/cache/clear")]
pub async fn clear_puzzle_cache(claims: Claims) -> Result<HttpResponse, AppError> {
    // Check if user is a game admin
    if !claims.is_game_admin() {
        return Err(AppError::Forbidden(
            "Only game administrators can clear the cache".to_string(),
        ));
    }

    clear_answer_cache();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Puzzle answer cache cleared"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_puzzle_request_parsing() {
        let json = r#"{"date": "2026-02-15", "word": "crane"}"#;
        let req: SetPuzzleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.date, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
        assert_eq!(req.word, Some("crane".to_string()));
        assert!(req.team_id.is_none());
        assert!(!req.set_random_unused_word);
    }

    #[test]
    fn test_set_puzzle_request_with_team_id() {
        let json = r#"{"date": "2026-02-15", "word": "crane", "teamId": "team-123"}"#;
        let req: SetPuzzleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.team_id, Some("team-123".to_string()));
    }

    #[test]
    fn test_set_puzzle_request_with_random_word() {
        let json = r#"{"date": "2026-02-15", "set_random_unused_word": true}"#;
        let req: SetPuzzleRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.date, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
        assert!(req.word.is_none());
        assert!(req.set_random_unused_word);
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

    #[test]
    fn test_puzzle_answer_response_with_word() {
        let response = PuzzleAnswerResponse {
            date: NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
            word: Some("crane".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("2026-02-15"));
        assert!(json.contains("crane"));
        assert!(json.contains("word"));
    }

    #[test]
    fn test_puzzle_answer_response_without_word() {
        let response = PuzzleAnswerResponse {
            date: NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
            word: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("2026-02-15"));
        assert!(!json.contains("word")); // word field should be omitted entirely
    }

    #[test]
    fn test_get_puzzles_query_empty_dates() {
        // Empty strings for dates should parse as None (this was the bug)
        let query: GetPuzzlesQuery =
            serde_urlencoded::from_str("start_date=&end_date=&omit_answers=true").unwrap();
        assert!(query.start_date.is_none());
        assert!(query.end_date.is_none());
        assert!(query.omit_answers);
    }

    #[test]
    fn test_get_puzzles_query_valid_dates() {
        let query: GetPuzzlesQuery =
            serde_urlencoded::from_str("start_date=2026-01-01&end_date=2026-01-31").unwrap();
        assert_eq!(
            query.start_date,
            Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap())
        );
        assert_eq!(
            query.end_date,
            Some(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap())
        );
        assert!(!query.omit_answers);
    }

    #[test]
    fn test_get_puzzles_query_missing_dates() {
        // Missing parameters should parse as None
        let query: GetPuzzlesQuery = serde_urlencoded::from_str("omit_answers=false").unwrap();
        assert!(query.start_date.is_none());
        assert!(query.end_date.is_none());
        assert!(!query.omit_answers);
    }

    #[test]
    fn test_get_puzzles_query_no_params() {
        // Empty query string should work
        let query: GetPuzzlesQuery = serde_urlencoded::from_str("").unwrap();
        assert!(query.start_date.is_none());
        assert!(query.end_date.is_none());
        assert!(!query.omit_answers);
    }

    #[test]
    fn test_validate_both_dates() {
        let query: GetPuzzlesQuery =
            serde_urlencoded::from_str("start_date=2026-01-01&end_date=2026-01-31").unwrap();
        assert!(query.validate().is_ok());
    }

    #[test]
    fn test_validate_only_start_date() {
        let query: GetPuzzlesQuery = serde_urlencoded::from_str("start_date=2026-01-01").unwrap();
        assert!(query.validate().is_err());
    }

    #[test]
    fn test_validate_only_end_date() {
        let query: GetPuzzlesQuery = serde_urlencoded::from_str("end_date=2026-01-31").unwrap();
        assert!(query.validate().is_err());
    }
}
