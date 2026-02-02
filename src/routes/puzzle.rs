//! Route handler for puzzle management (admin-only).

use actix_web::{get, put, web, HttpResponse};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;

use crate::db::PuzzleDatabase;
use crate::middleware::auth::Claims;
use crate::models::error::AppError;

/// Custom deserializer for optional NaiveDate that treats empty strings as None.
/// This is needed for query parameter parsing where empty values are passed as empty strings.
fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

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
    pub fn validate_date_range(&self) -> Result<(), &'static str> {
        match (self.start_date, self.end_date) {
            (Some(_), None) => Err("end_date is required when start_date is provided"),
            (None, Some(_)) => Err("start_date is required when end_date is provided"),
            _ => Ok(()),
        }
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

/// GET /puzzles/{date} - Get a single puzzle answer by date (game admin only).
///
/// This endpoint allows game administrators to retrieve a single puzzle answer
/// for a specific date. Only users with app_metadata.game_admin = true can access
/// this endpoint.
#[get("/puzzles/{date}")]
pub async fn get_puzzle_by_date(
    claims: Claims,
    path: web::Path<NaiveDate>,
    puzzle_db: web::Data<Arc<dyn PuzzleDatabase>>,
) -> Result<HttpResponse, AppError> {
    // Check if user is a game admin
    if !claims.is_game_admin() {
        return Err(AppError::Forbidden(
            "Only game administrators can view puzzle answers".to_string(),
        ));
    }

    let date = path.into_inner();
    let answer = puzzle_db
        .get_puzzle_answer(date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    match answer {
        Some(word) => Ok(HttpResponse::Ok().json(PuzzleAnswerResponse {
            date,
            word: Some(word),
        })),
        None => Err(AppError::not_found(format!(
            "No puzzle found for date {}",
            date
        ))),
    }
}

/// GET /puzzles - Get puzzle answers (game admin only).
///
/// This endpoint allows game administrators to retrieve puzzle answers.
/// With no query parameters, returns all puzzles. Use start_date and end_date
/// to filter by date range. Use omit_answers=true to only return dates without
/// the answer words. Only users with app_metadata.game_admin = true can access
/// this endpoint.
#[get("/puzzles")]
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

    // Validate that both dates are provided if either is specified
    if let Err(msg) = query.validate_date_range() {
        return Err(AppError::bad_request(msg));
    }

    let puzzles = puzzle_db
        .get_puzzle_answers(query.start_date, query.end_date)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let omit_answers = query.omit_answers;
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
    fn test_validate_date_range_both_provided() {
        let query: GetPuzzlesQuery =
            serde_urlencoded::from_str("start_date=2026-01-01&end_date=2026-01-31").unwrap();
        assert!(query.validate_date_range().is_ok());
    }

    #[test]
    fn test_validate_date_range_neither_provided() {
        let query: GetPuzzlesQuery = serde_urlencoded::from_str("").unwrap();
        assert!(query.validate_date_range().is_ok());
    }

    #[test]
    fn test_validate_date_range_only_start_date() {
        let query: GetPuzzlesQuery =
            serde_urlencoded::from_str("start_date=2026-01-01").unwrap();
        let result = query.validate_date_range();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "end_date is required when start_date is provided"
        );
    }

    #[test]
    fn test_validate_date_range_only_end_date() {
        let query: GetPuzzlesQuery =
            serde_urlencoded::from_str("end_date=2026-01-31").unwrap();
        let result = query.validate_date_range();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "start_date is required when end_date is provided"
        );
    }
}
