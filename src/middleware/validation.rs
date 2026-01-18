//! Request validation middleware.

use actix_web::HttpRequest;
use uuid::Uuid;

use crate::models::AppError;

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
