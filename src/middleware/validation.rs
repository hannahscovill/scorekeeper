//! Request validation middleware.

use crate::models::error::ValidationDetail;
use crate::models::score::ScoreCreateList;
use crate::models::AppError;

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

/// Validates a list of score create requests.
/// Returns an error if the list is empty.
pub fn validate_score_create_list(scores: &ScoreCreateList) -> Result<(), AppError> {
    if scores.is_empty() {
        return Err(AppError::validation(vec![ValidationDetail {
            field: "scores".to_string(),
            message: "Score list cannot be empty".to_string(),
        }]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::score::ScoreCreate;

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
    fn test_validate_score_create_list_valid() {
        let scores: ScoreCreateList = vec![ScoreCreate::new(100), ScoreCreate::new(200)];
        assert!(validate_score_create_list(&scores).is_ok());
    }

    #[test]
    fn test_validate_score_create_list_empty() {
        let scores: ScoreCreateList = vec![];
        let result = validate_score_create_list(&scores);
        assert!(result.is_err());
        if let Err(AppError::ValidationError(details)) = result {
            assert_eq!(details.len(), 1);
            assert_eq!(details[0].field, "scores");
        } else {
            panic!("Expected ValidationError");
        }
    }
}
