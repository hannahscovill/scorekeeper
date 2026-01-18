//! Business logic services for the scorekeeper API.

use crate::models::Score;

/// Service for managing scores.
pub struct ScoreService;

impl ScoreService {
    /// Creates a new ScoreService instance.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new score.
    pub fn create_score(&self, home_team: String, away_team: String) -> Score {
        Score::new(home_team, away_team)
    }
}

impl Default for ScoreService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_score() {
        let service = ScoreService::new();
        let score = service.create_score("Home".to_string(), "Away".to_string());
        assert_eq!(score.home_team, "Home");
        assert_eq!(score.away_team, "Away");
    }
}
