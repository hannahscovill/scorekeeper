//! Database operations for word puzzle games.

use async_trait::async_trait;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use chrono::{DateTime, NaiveDate, Utc};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::instrument;

use crate::models::guess::{GameState, PuzzleAnswer};

use super::traits::{DatabaseError, DatabaseResult};

/// Cache for puzzle answers (puzzle_date -> word).
/// Answers rarely change, so we cache them indefinitely.
static ANSWER_CACHE: Lazy<RwLock<HashMap<NaiveDate, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Trait for puzzle database operations.
#[async_trait]
pub trait PuzzleDatabase: Send + Sync {
    /// Gets the game state for a user on a specific puzzle date.
    async fn get_game_state(
        &self,
        user_id: &str,
        puzzle_date: NaiveDate,
    ) -> DatabaseResult<Option<GameState>>;

    /// Gets all game states for a user.
    async fn get_user_game_states(&self, user_id: &str) -> DatabaseResult<Vec<GameState>>;

    /// Creates or updates a game state.
    async fn upsert_game_state(&self, game_state: &GameState) -> DatabaseResult<GameState>;

    /// Deletes a game state for a user on a specific puzzle date.
    async fn delete_game_state(&self, user_id: &str, puzzle_date: NaiveDate) -> DatabaseResult<()>;

    /// Gets the puzzle answer for a specific date.
    async fn get_puzzle_answer(&self, puzzle_date: NaiveDate) -> DatabaseResult<Option<String>>;

    /// Gets all puzzle answers, optionally filtered by date range.
    async fn get_puzzle_answers(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> DatabaseResult<Vec<PuzzleAnswer>>;

    /// Sets the puzzle answer for a specific date.
    async fn set_puzzle_answer(
        &self,
        puzzle_date: NaiveDate,
        word: &str,
        team_id: Option<&str>,
    ) -> DatabaseResult<()>;
}

/// DynamoDB implementation of puzzle database.
pub struct DynamoDbPuzzleRepository {
    client: Client,
    table_name: String,
}

impl DynamoDbPuzzleRepository {
    /// Creates a new DynamoDB puzzle repository.
    pub fn new(client: Client, table_name: String) -> Self {
        Self { client, table_name }
    }

    /// Converts a GameState to DynamoDB item attributes.
    fn game_state_to_item(state: &GameState) -> HashMap<String, AttributeValue> {
        let mut item = HashMap::new();

        item.insert("pk".to_string(), AttributeValue::S(state.pk()));
        item.insert(
            "sk".to_string(),
            AttributeValue::S(GameState::sk().to_string()),
        );
        item.insert(
            "user_id".to_string(),
            AttributeValue::S(state.user_id.clone()),
        );
        item.insert(
            "puzzle_date".to_string(),
            AttributeValue::S(state.puzzle_date.to_string()),
        );
        item.insert(
            "guesses".to_string(),
            AttributeValue::L(
                state
                    .guesses
                    .iter()
                    .map(|g| AttributeValue::S(g.clone()))
                    .collect(),
            ),
        );
        item.insert("won".to_string(), AttributeValue::Bool(state.won));
        item.insert(
            "created_at".to_string(),
            AttributeValue::S(state.created_at.to_rfc3339()),
        );
        item.insert(
            "updated_at".to_string(),
            AttributeValue::S(state.updated_at.to_rfc3339()),
        );

        // Add TTL for anonymous (session cookie) users — 7 days from now.
        // Authenticated users have user_ids prefixed with "auth0|" (e.g., "auth0|abc123").
        if !state.user_id.contains("auth0|") {
            let ttl_epoch = (Utc::now() + chrono::Duration::days(7)).timestamp();
            item.insert("ttl".to_string(), AttributeValue::N(ttl_epoch.to_string()));
        }

        item
    }

    /// Converts DynamoDB item attributes to a GameState.
    fn item_to_game_state(item: &HashMap<String, AttributeValue>) -> DatabaseResult<GameState> {
        let user_id = item
            .get("user_id")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| DatabaseError::Other("Missing user_id".to_string()))?;

        let puzzle_date = item
            .get("puzzle_date")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .ok_or_else(|| DatabaseError::Other("Missing or invalid puzzle_date".to_string()))?;

        let guesses = item
            .get("guesses")
            .and_then(|v| v.as_l().ok())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_s().ok().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let won = item
            .get("won")
            .and_then(|v| v.as_bool().ok())
            .copied()
            .unwrap_or(false);

        let created_at = item
            .get("created_at")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let updated_at = item
            .get("updated_at")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Ok(GameState {
            user_id,
            puzzle_date,
            guesses,
            won,
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl PuzzleDatabase for DynamoDbPuzzleRepository {
    #[instrument(name = "db.get_game_state", skip(self))]
    async fn get_game_state(
        &self,
        user_id: &str,
        puzzle_date: NaiveDate,
    ) -> DatabaseResult<Option<GameState>> {
        let pk = format!("USER#{}#PUZZLE#{}", user_id, puzzle_date);
        let sk = GameState::sk();

        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk.to_string()))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB get error: {}", e)))?;

        match result.item {
            Some(item) => Ok(Some(Self::item_to_game_state(&item)?)),
            None => Ok(None),
        }
    }

    #[instrument(name = "db.get_user_game_states", skip(self))]
    async fn get_user_game_states(&self, user_id: &str) -> DatabaseResult<Vec<GameState>> {
        // Query using a begins_with on pk to find all USER#{user_id}#PUZZLE# entries
        // Since DynamoDB partition key is the full pk, we need to use a scan with filter
        // or create a GSI. For now, we'll use a scan with filter expression.
        let pk_prefix = format!("USER#{}#PUZZLE#", user_id);

        let result = self
            .client
            .scan()
            .table_name(&self.table_name)
            .filter_expression("begins_with(pk, :pk_prefix) AND sk = :sk")
            .expression_attribute_values(":pk_prefix", AttributeValue::S(pk_prefix))
            .expression_attribute_values(":sk", AttributeValue::S(GameState::sk().to_string()))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB scan error: {}", e)))?;

        let mut game_states = Vec::new();
        if let Some(items) = result.items {
            for item in items {
                game_states.push(Self::item_to_game_state(&item)?);
            }
        }

        // Sort by puzzle_date descending (most recent first)
        game_states.sort_by_key(|b| std::cmp::Reverse(b.puzzle_date));

        Ok(game_states)
    }

    #[instrument(name = "db.upsert_game_state", skip(self, game_state))]
    async fn upsert_game_state(&self, game_state: &GameState) -> DatabaseResult<GameState> {
        let item = Self::game_state_to_item(game_state);

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB put error: {}", e)))?;

        Ok(game_state.clone())
    }

    #[instrument(name = "db.delete_game_state", skip(self))]
    async fn delete_game_state(&self, user_id: &str, puzzle_date: NaiveDate) -> DatabaseResult<()> {
        let pk = format!("USER#{}#PUZZLE#{}", user_id, puzzle_date);
        let sk = GameState::sk();

        self.client
            .delete_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk.to_string()))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB delete error: {}", e)))?;

        Ok(())
    }

    #[instrument(name = "db.get_puzzle_answer", skip(self))]
    async fn get_puzzle_answer(&self, puzzle_date: NaiveDate) -> DatabaseResult<Option<String>> {
        // Check cache first
        {
            let cache = ANSWER_CACHE
                .read()
                .map_err(|e| DatabaseError::Other(format!("Cache lock error: {}", e)))?;
            if let Some(answer) = cache.get(&puzzle_date) {
                return Ok(Some(answer.clone()));
            }
        }

        // Fetch from DynamoDB
        let pk = PuzzleAnswer::pk(puzzle_date);
        let sk = PuzzleAnswer::sk();

        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk.to_string()))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB get error: {}", e)))?;

        let answer = result
            .item
            .and_then(|item| item.get("word").and_then(|v| v.as_s().ok()).cloned());

        // Cache the answer if found
        if let Some(ref word) = answer {
            if let Ok(mut cache) = ANSWER_CACHE.write() {
                cache.insert(puzzle_date, word.clone());
            }
        }

        Ok(answer)
    }

    #[instrument(name = "db.get_puzzle_answers", skip(self))]
    async fn get_puzzle_answers(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> DatabaseResult<Vec<PuzzleAnswer>> {
        // Scan for all PUZZLE# items with sk = ANSWER
        let mut builder = self
            .client
            .scan()
            .table_name(&self.table_name)
            .filter_expression("begins_with(pk, :pk_prefix) AND sk = :sk")
            .expression_attribute_values(":pk_prefix", AttributeValue::S("PUZZLE#".to_string()))
            .expression_attribute_values(":sk", AttributeValue::S(PuzzleAnswer::sk().to_string()));

        // Add date range filters if specified
        if start_date.is_some() || end_date.is_some() {
            let mut filter_parts = vec!["begins_with(pk, :pk_prefix)", "sk = :sk"];

            if let Some(start) = start_date {
                filter_parts.push("puzzle_date >= :start_date");
                builder = builder.expression_attribute_values(
                    ":start_date",
                    AttributeValue::S(start.to_string()),
                );
            }

            if let Some(end) = end_date {
                filter_parts.push("puzzle_date <= :end_date");
                builder = builder
                    .expression_attribute_values(":end_date", AttributeValue::S(end.to_string()));
            }

            builder = builder.filter_expression(filter_parts.join(" AND "));
        }

        let result = builder
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB scan error: {}", e)))?;

        let mut puzzles = Vec::new();
        if let Some(items) = result.items {
            for item in items {
                if let (Some(date_str), Some(word)) = (
                    item.get("puzzle_date").and_then(|v| v.as_s().ok()),
                    item.get("word").and_then(|v| v.as_s().ok()),
                ) {
                    if let Ok(puzzle_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                        puzzles.push(PuzzleAnswer {
                            puzzle_date,
                            word: word.clone(),
                        });
                    }
                }
            }
        }

        // Sort by date ascending
        puzzles.sort_by_key(|a| a.puzzle_date);

        Ok(puzzles)
    }

    #[instrument(name = "db.set_puzzle_answer", skip(self, word))]
    async fn set_puzzle_answer(
        &self,
        puzzle_date: NaiveDate,
        word: &str,
        team_id: Option<&str>,
    ) -> DatabaseResult<()> {
        let pk = PuzzleAnswer::pk(puzzle_date);
        let sk = PuzzleAnswer::sk();

        let mut item = HashMap::new();
        item.insert("pk".to_string(), AttributeValue::S(pk));
        item.insert("sk".to_string(), AttributeValue::S(sk.to_string()));
        item.insert("word".to_string(), AttributeValue::S(word.to_string()));
        item.insert(
            "puzzle_date".to_string(),
            AttributeValue::S(puzzle_date.to_string()),
        );

        if let Some(tid) = team_id {
            item.insert("team_id".to_string(), AttributeValue::S(tid.to_string()));
        }

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB put error: {}", e)))?;

        // Invalidate cache for this date
        if let Ok(mut cache) = ANSWER_CACHE.write() {
            cache.insert(puzzle_date, word.to_string());
        }

        Ok(())
    }
}

/// In-memory puzzle database for testing.
pub struct InMemoryPuzzleDb {
    game_states: RwLock<HashMap<String, GameState>>,
    puzzle_answers: RwLock<HashMap<NaiveDate, String>>,
}

impl InMemoryPuzzleDb {
    /// Creates a new in-memory puzzle database.
    pub fn new() -> Self {
        Self {
            game_states: RwLock::new(HashMap::new()),
            puzzle_answers: RwLock::new(HashMap::new()),
        }
    }

    /// Seeds a puzzle answer (for testing).
    pub fn seed_answer(&self, date: NaiveDate, word: impl Into<String>) {
        if let Ok(mut answers) = self.puzzle_answers.write() {
            answers.insert(date, word.into());
        }
    }
}

impl Default for InMemoryPuzzleDb {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PuzzleDatabase for InMemoryPuzzleDb {
    async fn get_game_state(
        &self,
        user_id: &str,
        puzzle_date: NaiveDate,
    ) -> DatabaseResult<Option<GameState>> {
        let key = format!("{}#{}", user_id, puzzle_date);
        let states = self
            .game_states
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        Ok(states.get(&key).cloned())
    }

    async fn get_user_game_states(&self, user_id: &str) -> DatabaseResult<Vec<GameState>> {
        let states = self
            .game_states
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;

        let mut user_states: Vec<GameState> = states
            .values()
            .filter(|s| s.user_id == user_id)
            .cloned()
            .collect();

        // Sort by puzzle_date descending (most recent first)
        user_states.sort_by_key(|b| std::cmp::Reverse(b.puzzle_date));

        Ok(user_states)
    }

    async fn upsert_game_state(&self, game_state: &GameState) -> DatabaseResult<GameState> {
        let key = format!("{}#{}", game_state.user_id, game_state.puzzle_date);
        let mut states = self
            .game_states
            .write()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        states.insert(key, game_state.clone());
        Ok(game_state.clone())
    }

    async fn delete_game_state(&self, user_id: &str, puzzle_date: NaiveDate) -> DatabaseResult<()> {
        let key = format!("{}#{}", user_id, puzzle_date);
        let mut states = self
            .game_states
            .write()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        states.remove(&key);
        Ok(())
    }

    async fn get_puzzle_answer(&self, puzzle_date: NaiveDate) -> DatabaseResult<Option<String>> {
        let answers = self
            .puzzle_answers
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        Ok(answers.get(&puzzle_date).cloned())
    }

    async fn get_puzzle_answers(
        &self,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> DatabaseResult<Vec<PuzzleAnswer>> {
        let answers = self
            .puzzle_answers
            .read()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;

        let mut puzzles: Vec<PuzzleAnswer> = answers
            .iter()
            .filter(|(date, _)| {
                let after_start = start_date.is_none_or(|start| **date >= start);
                let before_end = end_date.is_none_or(|end| **date <= end);
                after_start && before_end
            })
            .map(|(date, word)| PuzzleAnswer {
                puzzle_date: *date,
                word: word.clone(),
            })
            .collect();

        // Sort by date ascending
        puzzles.sort_by_key(|a| a.puzzle_date);

        Ok(puzzles)
    }

    async fn set_puzzle_answer(
        &self,
        puzzle_date: NaiveDate,
        word: &str,
        _team_id: Option<&str>,
    ) -> DatabaseResult<()> {
        let mut answers = self
            .puzzle_answers
            .write()
            .map_err(|e| DatabaseError::LockError(e.to_string()))?;
        answers.insert(puzzle_date, word.to_string());
        Ok(())
    }
}

/// Clears the answer cache (useful for testing).
pub fn clear_answer_cache() {
    if let Ok(mut cache) = ANSWER_CACHE.write() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inmemory_game_state_roundtrip() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        // Initially no state
        let state = db.get_game_state("user1", date).await.unwrap();
        assert!(state.is_none());

        // Create state
        let mut new_state = GameState::new("user1", date);
        new_state.add_guess("crane");
        db.upsert_game_state(&new_state).await.unwrap();

        // Retrieve state
        let retrieved = db.get_game_state("user1", date).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.guesses, vec!["crane"]);
    }

    #[tokio::test]
    async fn test_inmemory_puzzle_answer() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        // No answer initially
        let answer = db.get_puzzle_answer(date).await.unwrap();
        assert!(answer.is_none());

        // Seed answer
        db.seed_answer(date, "crane");

        // Retrieve answer
        let answer = db.get_puzzle_answer(date).await.unwrap();
        assert_eq!(answer, Some("crane".to_string()));
    }

    #[tokio::test]
    async fn test_inmemory_multiple_users() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let mut state1 = GameState::new("user1", date);
        state1.add_guess("crane");

        let mut state2 = GameState::new("user2", date);
        state2.add_guess("slate");
        state2.add_guess("moist");

        db.upsert_game_state(&state1).await.unwrap();
        db.upsert_game_state(&state2).await.unwrap();

        let retrieved1 = db.get_game_state("user1", date).await.unwrap().unwrap();
        let retrieved2 = db.get_game_state("user2", date).await.unwrap().unwrap();

        assert_eq!(retrieved1.guesses.len(), 1);
        assert_eq!(retrieved2.guesses.len(), 2);
    }

    #[tokio::test]
    async fn test_inmemory_update_state() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let mut state = GameState::new("user1", date);
        state.add_guess("crane");
        db.upsert_game_state(&state).await.unwrap();

        // Update with more guesses
        state.add_guess("slate");
        state.mark_won();
        db.upsert_game_state(&state).await.unwrap();

        let retrieved = db.get_game_state("user1", date).await.unwrap().unwrap();
        assert_eq!(retrieved.guesses.len(), 2);
        assert!(retrieved.won);
    }

    #[tokio::test]
    async fn test_inmemory_set_puzzle_answer() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();

        // No answer initially
        let answer = db.get_puzzle_answer(date).await.unwrap();
        assert!(answer.is_none());

        // Set answer
        db.set_puzzle_answer(date, "crane", None).await.unwrap();

        // Retrieve answer
        let answer = db.get_puzzle_answer(date).await.unwrap();
        assert_eq!(answer, Some("crane".to_string()));

        // Update answer
        db.set_puzzle_answer(date, "slate", Some("team-123"))
            .await
            .unwrap();

        let answer = db.get_puzzle_answer(date).await.unwrap();
        assert_eq!(answer, Some("slate".to_string()));
    }

    #[tokio::test]
    async fn test_inmemory_delete_game_state() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let mut state = GameState::new("user1", date);
        state.add_guess("crane");
        db.upsert_game_state(&state).await.unwrap();

        assert!(db.get_game_state("user1", date).await.unwrap().is_some());

        db.delete_game_state("user1", date).await.unwrap();

        assert!(db.get_game_state("user1", date).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_inmemory_delete_game_state_nonexistent() {
        let db = InMemoryPuzzleDb::new();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        // Deleting a nonexistent state should succeed (idempotent)
        db.delete_game_state("user1", date).await.unwrap();
    }
}
