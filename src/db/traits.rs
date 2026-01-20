//! Database trait abstractions for the scorekeeper API.

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::models::Game;

/// Errors that can occur during database operations.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// A lock could not be acquired.
    #[error("Lock error: {0}")]
    LockError(String),

    /// An item was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// A connection error occurred.
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// A general database error occurred.
    #[error("Database error: {0}")]
    Other(String),
}

/// Result type for database operations.
pub type DatabaseResult<T> = Result<T, DatabaseError>;

/// Trait for game database operations.
///
/// This trait abstracts the storage layer, allowing different implementations
/// such as in-memory storage for testing or DynamoDB for production.
#[async_trait]
pub trait GameDatabase: Send + Sync {
    /// Inserts a game into the database.
    async fn insert_game(&self, game: Game) -> DatabaseResult<Game>;

    /// Gets a game by its unique ID.
    async fn get_game(&self, id: &Uuid) -> DatabaseResult<Option<Game>>;

    /// Gets all games in the database.
    async fn get_all_games(&self) -> DatabaseResult<Vec<Game>>;

    /// Deletes a game by its unique ID.
    async fn delete_game(&self, id: &Uuid) -> DatabaseResult<Option<Game>>;

    /// Gets all games for a specific game session, optionally filtered by team.
    async fn get_games_by_game_id(
        &self,
        game_id: Uuid,
        team_id: Option<Uuid>,
    ) -> DatabaseResult<Vec<Game>>;
}
