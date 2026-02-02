//! Request validation middleware.

use actix_web::HttpRequest;
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer};
use uuid::Uuid;

use crate::models::error::ValidationDetail;
use crate::models::game::GameCreateList;
use crate::models::AppError;

/// Custom deserializer for optional NaiveDate that treats empty strings as None.
/// This is needed for query parameter parsing where empty values are passed as empty strings.
///
/// # Example
/// ```ignore
/// #[derive(Deserialize)]
/// struct QueryParams {
///     #[serde(default, deserialize_with = "deserialize_optional_date")]
///     start_date: Option<NaiveDate>,
/// }
/// ```
pub fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
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

/// Validates that a date range has both start and end dates, or neither.
/// Returns an error message if only one date is provided.
///
/// # Example
/// ```ignore
/// if let Err(msg) = validate_date_range(query.start_date, query.end_date) {
///     return Err(AppError::bad_request(msg));
/// }
/// ```
pub fn validate_date_range(
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
) -> Result<(), &'static str> {
    match (start_date, end_date) {
        (Some(_), None) => Err("end_date is required when start_date is provided"),
        (None, Some(_)) => Err("start_date is required when end_date is provided"),
        _ => Ok(()),
    }
}

/// Extracts and validates the optional team-id header from a request.
pub fn extract_team_id_header(req: &HttpRequest) -> Result<Option<Uuid>, AppError> {
    match req.headers().get("team-id") {
        Some(header_value) => {
            let header_str = header_value.to_str().map_err(|_| {
                AppError::bad_request("Invalid team-id header: contains non-ASCII characters")
            })?;

            let team_id = Uuid::parse_str(header_str)
                .map_err(|_| AppError::bad_request("Invalid team-id header: not a valid UUID"))?;

            Ok(Some(team_id))
        }
        None => Ok(None),
    }
}

/// Validates that a team name is not empty.
pub fn validate_team_name(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Team name cannot be empty".to_string(),
        ));
    }
    Ok(())
}

/// Validates a score value.
pub fn validate_score(score: u32) -> Result<(), AppError> {
    // Scores are u32, so they're always non-negative
    // Add any additional validation rules here
    let _ = score;
    Ok(())
}

/// Validates a list of game create requests.
/// Returns an error if the list is empty.
pub fn validate_game_create_list(games: &GameCreateList) -> Result<(), AppError> {
    if games.is_empty() {
        return Err(AppError::validation(vec![ValidationDetail {
            field: "games".to_string(),
            message: "Game list cannot be empty".to_string(),
        }]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::game::GameCreate;
    use actix_web::test::TestRequest;

    #[test]
    fn test_validate_team_name_valid() {
        assert!(validate_team_name("Team A").is_ok());
    }

    #[test]
    fn test_validate_team_name_empty() {
        assert!(validate_team_name("").is_err());
        assert!(validate_team_name("   ").is_err());
    }

    #[test]
    fn test_validate_score() {
        assert!(validate_score(0).is_ok());
        assert!(validate_score(100).is_ok());
    }

    #[test]
    fn test_extract_team_id_header_valid() {
        let team_id = Uuid::new_v4();
        let req = TestRequest::default()
            .insert_header(("team-id", team_id.to_string()))
            .to_http_request();

        let result = extract_team_id_header(&req).unwrap();
        assert_eq!(result, Some(team_id));
    }

    #[test]
    fn test_extract_team_id_header_missing() {
        let req = TestRequest::default().to_http_request();

        let result = extract_team_id_header(&req).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_team_id_header_invalid_uuid() {
        let req = TestRequest::default()
            .insert_header(("team-id", "not-a-uuid"))
            .to_http_request();

        let result = extract_team_id_header(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_team_id_header_empty_string() {
        let req = TestRequest::default()
            .insert_header(("team-id", ""))
            .to_http_request();

        let result = extract_team_id_header(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_game_create_list_valid() {
        let games: GameCreateList = vec![GameCreate::new(100), GameCreate::new(200)];
        assert!(validate_game_create_list(&games).is_ok());
    }

    #[test]
    fn test_validate_game_create_list_empty() {
        let games: GameCreateList = vec![];
        let result = validate_game_create_list(&games);
        assert!(result.is_err());
        if let Err(AppError::ValidationError(details)) = result {
            assert_eq!(details.len(), 1);
            assert_eq!(details[0].field, "games");
        } else {
            panic!("Expected ValidationError");
        }
    }

    #[test]
    fn test_validate_game_create_list_single_item() {
        let games: GameCreateList = vec![GameCreate::new(50)];
        assert!(validate_game_create_list(&games).is_ok());
    }

    #[test]
    fn test_validate_game_create_list_many_items() {
        let games: GameCreateList = (0..100).map(|i| GameCreate::new(i)).collect();
        assert!(validate_game_create_list(&games).is_ok());
    }

    #[test]
    fn test_validate_date_range_both_provided() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1);
        let end = NaiveDate::from_ymd_opt(2026, 1, 31);
        assert!(validate_date_range(start, end).is_ok());
    }

    #[test]
    fn test_validate_date_range_neither_provided() {
        assert!(validate_date_range(None, None).is_ok());
    }

    #[test]
    fn test_validate_date_range_only_start() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1);
        let result = validate_date_range(start, None);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "end_date is required when start_date is provided"
        );
    }

    #[test]
    fn test_validate_date_range_only_end() {
        let end = NaiveDate::from_ymd_opt(2026, 1, 31);
        let result = validate_date_range(None, end);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "start_date is required when end_date is provided"
        );
    }
}
