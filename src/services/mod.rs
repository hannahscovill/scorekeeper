//! Business logic services for the scorekeeper API.

use crate::models::{Score, ScoreCreate};
use uuid::Uuid;

/// Service for managing scores.
pub struct ScoreService;

impl ScoreService {
    /// Creates a new ScoreService instance.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new score from a ScoreCreate request.
    pub fn create_score(&self, user_id: Uuid, game_id: Uuid, create: &ScoreCreate) -> Score {
        Score::new(user_id, game_id, create.score)
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
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let create = ScoreCreate::new(100);
        let score = service.create_score(user_id, game_id, &create);
        assert_eq!(score.user_id, user_id);
        assert_eq!(score.game_id, game_id);
        assert_eq!(score.score, 100);
    }
}
