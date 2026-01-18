//! Score data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a score entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    /// Unique identifier for the score.
    pub id: Uuid,
    /// Name of the home team.
    pub home_team: String,
    /// Name of the away team.
    pub away_team: String,
    /// Home team's score.
    pub home_score: u32,
    /// Away team's score.
    pub away_score: u32,
    /// When the score was created.
    pub created_at: DateTime<Utc>,
    /// When the score was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Score {
    /// Creates a new score entry.
    pub fn new(home_team: String, away_team: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            home_team,
            away_team,
            home_score: 0,
            away_score: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Request payload for creating a new score.
#[derive(Debug, Deserialize)]
pub struct CreateScoreRequest {
    /// Name of the home team.
    pub home_team: String,
    /// Name of the away team.
    pub away_team: String,
}

/// Request payload for updating a score.
#[derive(Debug, Deserialize)]
pub struct UpdateScoreRequest {
    /// New home team score.
    pub home_score: Option<u32>,
    /// New away team score.
    pub away_score: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_score() {
        let score = Score::new("Team A".to_string(), "Team B".to_string());
        assert_eq!(score.home_team, "Team A");
        assert_eq!(score.away_team, "Team B");
        assert_eq!(score.home_score, 0);
        assert_eq!(score.away_score, 0);
    }
}
