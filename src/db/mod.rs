//! Database layer for the scorekeeper API.

pub mod dynamodb;
pub mod traits;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

use crate::models::Game;

pub use dynamodb::DynamoDbRepository;
pub use traits::{DatabaseError, DatabaseResult, GameDatabase};

/// In-memory database for development and testing.
pub struct InMemoryDb {
    games: RwLock<HashMap<Uuid, Game>>,
}

impl InMemoryDb {
    /// Creates a new in-memory database.
    pub fn new() -> Self {
        Self {
            games: RwLock::new(HashMap::new()),
        }
    }

    /// Inserts a game into the database.
    pub fn insert_game(&self, game: Game) -> Result<Game, String> {
        let mut games = self
            .games
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        let id = game.id;
        games.insert(id, game.clone());
        Ok(game)
    }

    /// Gets a game by ID.
    pub fn get_game(&self, id: &Uuid) -> Result<Option<Game>, String> {
        let games = self
            .games
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(games.get(id).cloned())
    }

    /// Gets all games.
    pub fn get_all_games(&self) -> Result<Vec<Game>, String> {
        let games = self
            .games
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(games.values().cloned().collect())
    }

    /// Deletes a game by ID.
    pub fn delete_game(&self, id: &Uuid) -> Result<Option<Game>, String> {
        let mut games = self
            .games
            .write()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(games.remove(id))
    }

    /// Gets all games for a specific game session, optionally filtered by team.
    pub fn get_games_by_game_id(
        &self,
        game_id: Uuid,
        team_id: Option<Uuid>,
    ) -> Result<Vec<Game>, String> {
        let games = self
            .games
            .read()
            .map_err(|e| format!("Lock error: {}", e))?;
        Ok(games
            .values()
            .filter(|g| g.game_id == game_id)
            .filter(|g| team_id.is_none_or(|tid| g.team_id == Some(tid)))
            .cloned()
            .collect())
    }
}

impl Default for InMemoryDb {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GameDatabase for InMemoryDb {
    async fn insert_game(&self, game: Game) -> DatabaseResult<Game> {
        let mut games = self
            .games
            .write()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        let id = game.id;
        games.insert(id, game.clone());
        Ok(game)
    }

    async fn get_game(&self, id: &Uuid) -> DatabaseResult<Option<Game>> {
        let games = self
            .games
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        Ok(games.get(id).cloned())
    }

    async fn get_all_games(&self) -> DatabaseResult<Vec<Game>> {
        let games = self
            .games
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        Ok(games.values().cloned().collect())
    }

    async fn delete_game(&self, id: &Uuid) -> DatabaseResult<Option<Game>> {
        let mut games = self
            .games
            .write()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        Ok(games.remove(id))
    }

    async fn get_games_by_game_id(
        &self,
        game_id: Uuid,
        team_id: Option<Uuid>,
    ) -> DatabaseResult<Vec<Game>> {
        let games = self
            .games
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        Ok(games
            .values()
            .filter(|g| g.game_id == game_id)
            .filter(|g| team_id.is_none_or(|tid| g.team_id == Some(tid)))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get_game() {
        let db = InMemoryDb::new();
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let game = Game::new(user_id, game_id, 100);
        let id = game.id;

        db.insert_game(game).unwrap();
        let retrieved = db.get_game(&id).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().score, 100);
    }

    #[test]
    fn test_delete_game() {
        let db = InMemoryDb::new();
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let game = Game::new(user_id, game_id, 200);
        let id = game.id;

        db.insert_game(game).unwrap();
        let deleted = db.delete_game(&id).unwrap();

        assert!(deleted.is_some());
        assert!(db.get_game(&id).unwrap().is_none());
    }

    #[test]
    fn test_get_games_by_game_id() {
        let db = InMemoryDb::new();
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let other_game_id = Uuid::new_v4();

        let game1 = Game::new(user_id, game_id, 100);
        let game2 = Game::new(user_id, game_id, 200);
        let game3 = Game::new(user_id, other_game_id, 300);

        db.insert_game(game1).unwrap();
        db.insert_game(game2).unwrap();
        db.insert_game(game3).unwrap();

        let games = db.get_games_by_game_id(game_id, None).unwrap();
        assert_eq!(games.len(), 2);
        assert!(games.iter().all(|g| g.game_id == game_id));
    }

    #[test]
    fn test_get_games_by_game_id_with_team_filter() {
        let db = InMemoryDb::new();
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let other_team_id = Uuid::new_v4();

        let game1 = Game::with_team(user_id, game_id, team_id, 100);
        let game2 = Game::with_team(user_id, game_id, other_team_id, 200);
        let game3 = Game::new(user_id, game_id, 300); // No team

        db.insert_game(game1).unwrap();
        db.insert_game(game2).unwrap();
        db.insert_game(game3).unwrap();

        let games = db.get_games_by_game_id(game_id, Some(team_id)).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].team_id, Some(team_id));
    }

    #[test]
    fn test_get_games_by_game_id_empty() {
        let db = InMemoryDb::new();
        let game_id = Uuid::new_v4();

        let games = db.get_games_by_game_id(game_id, None).unwrap();
        assert!(games.is_empty());
    }
}
