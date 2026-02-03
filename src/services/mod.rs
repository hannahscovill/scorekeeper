//! Business logic services for the scorekeeper API.

pub mod auth0;
pub mod common_words;
pub mod grading;
pub mod s3_avatar;

use crate::models::{Game, GameCreate};
use uuid::Uuid;

pub use auth0::Auth0ManagementService;
pub use common_words::CommonWordsService;
pub use grading::{grade_guess, is_winning_guess};
pub use s3_avatar::S3AvatarService;

/// Service for managing games.
pub struct GameService;

impl GameService {
    /// Creates a new GameService instance.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new game from a GameCreate request.
    pub fn create_game(
        &self,
        user_id: impl Into<String>,
        game_id: Uuid,
        create: &GameCreate,
    ) -> Game {
        Game::new(user_id, game_id, create.score)
    }
}

impl Default for GameService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_game() {
        let service = GameService::new();
        let user_id = "auth0|testuser";
        let game_id = Uuid::new_v4();
        let create = GameCreate::new(100);
        let game = service.create_game(user_id, game_id, &create);
        assert_eq!(game.user_id, user_id);
        assert_eq!(game.game_id, game_id);
        assert_eq!(game.score, 100);
    }
}
