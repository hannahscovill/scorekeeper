//! Database layer for the scorekeeper API.

use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

use crate::models::Score;

/// In-memory database for development and testing.
pub struct InMemoryDb {
    scores: RwLock<HashMap<Uuid, Score>>,
}

impl InMemoryDb {
    /// Creates a new in-memory database.
    pub fn new() -> Self {
        Self {
            scores: RwLock::new(HashMap::new()),
        }
    }

    /// Inserts a score into the database.
    pub fn insert_score(&self, score: Score) -> Result<Score, String> {
        let mut scores = self
            .scores
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        let id = score.id;
        scores.insert(id, score.clone());
        Ok(score)
    }

    /// Gets a score by ID.
    pub fn get_score(&self, id: &Uuid) -> Result<Option<Score>, String> {
        let scores = self
            .scores
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(scores.get(id).cloned())
    }

    /// Gets all scores.
    pub fn get_all_scores(&self) -> Result<Vec<Score>, String> {
        let scores = self
            .scores
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(scores.values().cloned().collect())
    }

    /// Deletes a score by ID.
    pub fn delete_score(&self, id: &Uuid) -> Result<Option<Score>, String> {
        let mut scores = self
            .scores
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(scores.remove(id))
    }
}

impl Default for InMemoryDb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get_score() {
        let db = InMemoryDb::new();
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let score = Score::new(user_id, game_id, 100);
        let id = score.id;

        db.insert_score(score).unwrap();
        let retrieved = db.get_score(&id).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().score, 100);
    }

    #[test]
    fn test_delete_score() {
        let db = InMemoryDb::new();
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let score = Score::new(user_id, game_id, 200);
        let id = score.id;

        db.insert_score(score).unwrap();
        let deleted = db.delete_score(&id).unwrap();

        assert!(deleted.is_some());
        assert!(db.get_score(&id).unwrap().is_none());
    }
}
