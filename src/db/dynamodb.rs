//! DynamoDB database implementation.

use async_trait::async_trait;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::instrument;
use uuid::Uuid;

use crate::models::Game;

use super::traits::{DatabaseError, DatabaseResult, GameDatabase};

/// DynamoDB repository for game storage.
pub struct DynamoDbRepository {
    client: Client,
    table_name: String,
}

impl DynamoDbRepository {
    /// Creates a new DynamoDB repository.
    pub fn new(client: Client, table_name: String) -> Self {
        Self { client, table_name }
    }

    /// Converts a Game to DynamoDB item attributes.
    fn game_to_item(game: &Game) -> HashMap<String, AttributeValue> {
        let mut item = HashMap::new();

        // Primary key: pk = GAME#{id}, sk = GAME#{id}
        item.insert(
            "pk".to_string(),
            AttributeValue::S(format!("GAME#{}", game.id)),
        );
        item.insert(
            "sk".to_string(),
            AttributeValue::S(format!("GAME#{}", game.id)),
        );

        // Game attributes
        item.insert("id".to_string(), AttributeValue::S(game.id.to_string()));
        item.insert(
            "user_id".to_string(),
            AttributeValue::S(game.user_id.to_string()),
        );
        item.insert(
            "game_id".to_string(),
            AttributeValue::S(game.game_id.to_string()),
        );
        item.insert(
            "score".to_string(),
            AttributeValue::N(game.score.to_string()),
        );
        item.insert(
            "created_at".to_string(),
            AttributeValue::S(game.created_at.to_rfc3339()),
        );

        if let Some(team_id) = game.team_id {
            item.insert(
                "team_id".to_string(),
                AttributeValue::S(team_id.to_string()),
            );
        }

        item
    }

    /// Converts DynamoDB item attributes to a Game.
    fn item_to_game(item: &HashMap<String, AttributeValue>) -> DatabaseResult<Game> {
        let id = item
            .get("id")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| DatabaseError::Other("Missing or invalid id".to_string()))?;

        let user_id = item
            .get("user_id")
            .and_then(|v| v.as_s().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| DatabaseError::Other("Missing or invalid user_id".to_string()))?;

        let game_id = item
            .get("game_id")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| DatabaseError::Other("Missing or invalid game_id".to_string()))?;

        let score = item
            .get("score")
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<i32>().ok())
            .ok_or_else(|| DatabaseError::Other("Missing or invalid score".to_string()))?;

        let created_at = item
            .get("created_at")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .ok_or_else(|| DatabaseError::Other("Missing or invalid created_at".to_string()))?;

        let team_id = item
            .get("team_id")
            .and_then(|v| v.as_s().ok())
            .and_then(|s| Uuid::parse_str(s).ok());

        Ok(Game {
            id,
            user_id,
            game_id,
            team_id,
            score,
            created_at,
        })
    }
}

#[async_trait]
impl GameDatabase for DynamoDbRepository {
    #[instrument(name = "db.insert_game", skip(self, game))]
    async fn insert_game(&self, game: Game) -> DatabaseResult<Game> {
        let item = Self::game_to_item(&game);

        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB put error: {}", e)))?;

        Ok(game)
    }

    #[instrument(name = "db.get_game", skip(self))]
    async fn get_game(&self, id: &Uuid) -> DatabaseResult<Option<Game>> {
        let pk = format!("GAME#{}", id);
        let sk = format!("GAME#{}", id);

        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk))
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB get error: {}", e)))?;

        match result.item {
            Some(item) => Ok(Some(Self::item_to_game(&item)?)),
            None => Ok(None),
        }
    }

    #[instrument(name = "db.get_all_games", skip(self))]
    async fn get_all_games(&self) -> DatabaseResult<Vec<Game>> {
        let result = self
            .client
            .scan()
            .table_name(&self.table_name)
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB scan error: {}", e)))?;

        let items = result.items.unwrap_or_default();
        items.iter().map(Self::item_to_game).collect()
    }

    #[instrument(name = "db.delete_game", skip(self))]
    async fn delete_game(&self, id: &Uuid) -> DatabaseResult<Option<Game>> {
        let pk = format!("GAME#{}", id);
        let sk = format!("GAME#{}", id);

        // First get the item to return it
        let existing = self.get_game(id).await?;

        if existing.is_some() {
            self.client
                .delete_item()
                .table_name(&self.table_name)
                .key("pk", AttributeValue::S(pk))
                .key("sk", AttributeValue::S(sk))
                .send()
                .await
                .map_err(|e| DatabaseError::Other(format!("DynamoDB delete error: {}", e)))?;
        }

        Ok(existing)
    }

    #[instrument(name = "db.get_games_by_game_id", skip(self))]
    async fn get_games_by_game_id(
        &self,
        game_id: Uuid,
        team_id: Option<Uuid>,
    ) -> DatabaseResult<Vec<Game>> {
        // Use the GameSessionIndex GSI to query by game_id
        let mut query = self
            .client
            .query()
            .table_name(&self.table_name)
            .index_name("GameSessionIndex")
            .key_condition_expression("game_id = :gid")
            .expression_attribute_values(":gid", AttributeValue::S(game_id.to_string()));

        // Add team filter if provided
        if let Some(tid) = team_id {
            query = query
                .filter_expression("team_id = :tid")
                .expression_attribute_values(":tid", AttributeValue::S(tid.to_string()));
        }

        let result = query
            .send()
            .await
            .map_err(|e| DatabaseError::Other(format!("DynamoDB query error: {}", e)))?;

        let items = result.items.unwrap_or_default();
        items.iter().map(Self::item_to_game).collect()
    }
}
