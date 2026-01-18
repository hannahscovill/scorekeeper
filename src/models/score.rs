//! Score data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a score entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Score {
    /// Unique identifier for the score.
    pub id: Uuid,
    /// User who created the score.
    pub user_id: Uuid,
    /// Game this score belongs to.
    pub game_id: Uuid,
    /// Optional team identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<Uuid>,
    /// The score value.
    pub score: i32,
    /// When the score was created.
    pub created_at: DateTime<Utc>,
}

impl Score {
    /// Creates a new score entry.
    pub fn new(user_id: Uuid, game_id: Uuid, score: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            game_id,
            team_id: None,
            score,
            created_at: Utc::now(),
        }
    }

    /// Creates a new score entry with a team.
    pub fn with_team(user_id: Uuid, game_id: Uuid, team_id: Uuid, score: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            game_id,
            team_id: Some(team_id),
            score,
            created_at: Utc::now(),
        }
    }
}

/// Request payload for creating a new score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreCreate {
    /// The score value.
    pub score: i32,
    /// Optional game identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_id: Option<Uuid>,
}

impl ScoreCreate {
    /// Creates a new ScoreCreate with just a score.
    pub fn new(score: i32) -> Self {
        Self {
            score,
            game_id: None,
        }
    }

    /// Creates a new ScoreCreate with a score and game ID.
    pub fn with_game(score: i32, game_id: Uuid) -> Self {
        Self {
            score,
            game_id: Some(game_id),
        }
    }
}

/// A list of scores.
pub type ScoreList = Vec<Score>;

/// A list of score creation requests.
pub type ScoreCreateList = Vec<ScoreCreate>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_score() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let score = Score::new(user_id, game_id, 100);

        assert_eq!(score.user_id, user_id);
        assert_eq!(score.game_id, game_id);
        assert_eq!(score.score, 100);
        assert!(score.team_id.is_none());
    }

    #[test]
    fn test_score_with_team() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let score = Score::with_team(user_id, game_id, team_id, 200);

        assert_eq!(score.user_id, user_id);
        assert_eq!(score.game_id, game_id);
        assert_eq!(score.team_id, Some(team_id));
        assert_eq!(score.score, 200);
    }

    #[test]
    fn test_score_serialization() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let game_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let score = Score {
            id: Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap(),
            user_id,
            game_id,
            team_id: None,
            score: 150,
            created_at: DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string(&score).unwrap();
        // team_id should not be present when None
        assert!(!json.contains("team_id"));
        assert!(json.contains("\"score\":150"));
    }

    #[test]
    fn test_score_deserialization() {
        let json = r#"{
            "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "user_id": "550e8400-e29b-41d4-a716-446655440000",
            "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "score": 150,
            "created_at": "2024-01-15T10:30:00Z"
        }"#;

        let score: Score = serde_json::from_str(json).unwrap();
        assert_eq!(score.score, 150);
        assert!(score.team_id.is_none());
    }

    #[test]
    fn test_score_with_team_serialization() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let game_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let team_id = Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef1234567890").unwrap();
        let score = Score {
            id: Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap(),
            user_id,
            game_id,
            team_id: Some(team_id),
            score: 200,
            created_at: DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string(&score).unwrap();
        // team_id should be present when Some
        assert!(json.contains("team_id"));
        assert!(json.contains("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
    }

    #[test]
    fn test_score_create_serialization() {
        let create = ScoreCreate::new(100);
        let json = serde_json::to_string(&create).unwrap();
        // game_id should not be present when None
        assert!(!json.contains("game_id"));
        assert!(json.contains("\"score\":100"));
    }

    #[test]
    fn test_score_create_with_game_serialization() {
        let game_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let create = ScoreCreate::with_game(100, game_id);
        let json = serde_json::to_string(&create).unwrap();
        assert!(json.contains("game_id"));
        assert!(json.contains("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
    }

    #[test]
    fn test_score_create_deserialization() {
        let json = r#"{"score": 250}"#;
        let create: ScoreCreate = serde_json::from_str(json).unwrap();
        assert_eq!(create.score, 250);
        assert!(create.game_id.is_none());
    }

    #[test]
    fn test_score_list() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let scores: ScoreList = vec![
            Score::new(user_id, game_id, 100),
            Score::new(user_id, game_id, 200),
        ];
        assert_eq!(scores.len(), 2);
    }

    #[test]
    fn test_score_create_list() {
        let creates: ScoreCreateList = vec![ScoreCreate::new(100), ScoreCreate::new(200)];
        assert_eq!(creates.len(), 2);
    }
}
