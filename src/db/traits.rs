//! Database trait abstractions for the scorekeeper API.

use std::future::Future;
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
pub trait GameDatabase: Send + Sync {
    /// Inserts a game into the database.
    fn insert_game(&self, game: Game) -> impl Future<Output = DatabaseResult<Game>> + Send;

    /// Gets a game by its unique ID.
    fn get_game(&self, id: &Uuid) -> impl Future<Output = DatabaseResult<Option<Game>>> + Send;

    /// Gets all games in the database.
    fn get_all_games(&self) -> impl Future<Output = DatabaseResult<Vec<Game>>> + Send;

    /// Deletes a game by its unique ID.
    fn delete_game(&self, id: &Uuid) -> impl Future<Output = DatabaseResult<Option<Game>>> + Send;

    /// Gets all games for a specific game session, optionally filtered by team.
    fn get_games_by_game_id(
        &self,
        game_id: Uuid,
        team_id: Option<Uuid>,
    ) -> impl Future<Output = DatabaseResult<Vec<Game>>> + Send;
}
