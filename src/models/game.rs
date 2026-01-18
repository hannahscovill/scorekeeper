//! Game data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a game entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Game {
    /// Unique identifier for the game.
    pub id: Uuid,
    /// User who created the game.
    pub user_id: Uuid,
    /// Game session this game belongs to.
    pub game_id: Uuid,
    /// Optional team identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<Uuid>,
    /// The score value.
    pub score: i32,
    /// When the game was created.
    pub created_at: DateTime<Utc>,
}

impl Game {
    /// Creates a new game entry.
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

    /// Creates a new game entry with a team.
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

/// Request payload for creating a new game.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameCreate {
    /// The score value.
    pub score: i32,
    /// Optional game identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_id: Option<Uuid>,
}

impl GameCreate {
    /// Creates a new GameCreate with just a score.
    pub fn new(score: i32) -> Self {
        Self {
            score,
            game_id: None,
        }
    }

    /// Creates a new GameCreate with a score and game ID.
    pub fn with_game(score: i32, game_id: Uuid) -> Self {
        Self {
            score,
            game_id: Some(game_id),
        }
    }
}

/// A list of games.
pub type GameList = Vec<Game>;

/// A list of game creation requests.
pub type GameCreateList = Vec<GameCreate>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let game = Game::new(user_id, game_id, 100);

        assert_eq!(game.user_id, user_id);
        assert_eq!(game.game_id, game_id);
        assert_eq!(game.score, 100);
        assert!(game.team_id.is_none());
    }

    #[test]
    fn test_game_with_team() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let game = Game::with_team(user_id, game_id, team_id, 200);

        assert_eq!(game.user_id, user_id);
        assert_eq!(game.game_id, game_id);
        assert_eq!(game.team_id, Some(team_id));
        assert_eq!(game.score, 200);
    }

    #[test]
    fn test_game_serialization() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let game_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let game = Game {
            id: Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap(),
            user_id,
            game_id,
            team_id: None,
            score: 150,
            created_at: DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string(&game).unwrap();
        // team_id should not be present when None
        assert!(!json.contains("team_id"));
        assert!(json.contains("\"score\":150"));
    }

    #[test]
    fn test_game_deserialization() {
        let json = r#"{
            "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "user_id": "550e8400-e29b-41d4-a716-446655440000",
            "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "score": 150,
            "created_at": "2024-01-15T10:30:00Z"
        }"#;

        let game: Game = serde_json::from_str(json).unwrap();
        assert_eq!(game.score, 150);
        assert!(game.team_id.is_none());
    }

    #[test]
    fn test_game_with_team_serialization() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let game_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let team_id = Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef1234567890").unwrap();
        let game = Game {
            id: Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap(),
            user_id,
            game_id,
            team_id: Some(team_id),
            score: 200,
            created_at: DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string(&game).unwrap();
        // team_id should be present when Some
        assert!(json.contains("team_id"));
        assert!(json.contains("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
    }

    #[test]
    fn test_game_create_serialization() {
        let create = GameCreate::new(100);
        let json = serde_json::to_string(&create).unwrap();
        // game_id should not be present when None
        assert!(!json.contains("game_id"));
        assert!(json.contains("\"score\":100"));
    }

    #[test]
    fn test_game_create_with_game_serialization() {
        let game_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let create = GameCreate::with_game(100, game_id);
        let json = serde_json::to_string(&create).unwrap();
        assert!(json.contains("game_id"));
        assert!(json.contains("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
    }

    #[test]
    fn test_game_create_deserialization() {
        let json = r#"{"score": 250}"#;
        let create: GameCreate = serde_json::from_str(json).unwrap();
        assert_eq!(create.score, 250);
        assert!(create.game_id.is_none());
    }

    #[test]
    fn test_game_list() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let games: GameList = vec![
            Game::new(user_id, game_id, 100),
            Game::new(user_id, game_id, 200),
        ];
        assert_eq!(games.len(), 2);
    }

    #[test]
    fn test_game_create_list() {
        let creates: GameCreateList = vec![GameCreate::new(100), GameCreate::new(200)];
        assert_eq!(creates.len(), 2);
    }
}
