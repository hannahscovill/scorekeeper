//! Request validation middleware.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
