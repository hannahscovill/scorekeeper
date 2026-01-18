//! Secrets management module for secure configuration.
//!
//! This module provides abstractions for loading secrets from various sources
//! (AWS Secrets Manager, environment variables, etc.) with caching support.

mod aws_provider;
mod env_provider;
mod provider;

pub use aws_provider::AwsSecretsProvider;
pub use env_provider::EnvSecretsProvider;
pub use provider::{SecretValue, SecretsProvider};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A simple in-memory cache for secrets.
///
/// This cache stores secrets in memory to minimize repeated API calls.
/// In production, consider adding TTL and refresh logic.
#[derive(Clone)]
pub struct SecretsCache {
    cache: Arc<RwLock<HashMap<String, String>>>,
}

impl SecretsCache {
    /// Creates a new empty secrets cache.
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Gets a secret from the cache, if it exists.
    pub async fn get(&self, key: &str) -> Option<String> {
        let cache = self.cache.read().await;
        cache.get(key).cloned()
    }

    /// Stores a secret in the cache.
    pub async fn set(&self, key: String, value: String) {
        let mut cache = self.cache.write().await;
        cache.insert(key, value);
    }
}

impl Default for SecretsCache {
    fn default() -> Self {
        Self::new()
    }
}
