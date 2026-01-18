//! AWS Secrets Manager provider implementation.

use async_trait::async_trait;
use aws_sdk_secretsmanager::Client;
use tracing::{debug, warn};

use super::provider::{SecretValue, SecretsError, SecretsProvider};
use super::SecretsCache;

/// Secrets provider that loads secrets from AWS Secrets Manager.
///
/// This provider includes a simple in-memory cache to minimize API calls.
/// Secrets are cached indefinitely once loaded. For production use with
/// secret rotation, consider adding TTL and refresh logic.
pub struct AwsSecretsProvider {
    client: Client,
    cache: SecretsCache,
}

impl AwsSecretsProvider {
    /// Creates a new AWS Secrets Manager provider with the default AWS configuration.
    ///
    /// This will use the AWS SDK's default credential and region resolution,
    /// which checks environment variables, AWS config files, IAM roles, etc.
    pub async fn new() -> Self {
        let config = aws_config::load_from_env().await;
        let client = Client::new(&config);
        Self {
            client,
            cache: SecretsCache::new(),
        }
    }

    /// Creates a new AWS Secrets Manager provider with a custom client.
    ///
    /// Useful for testing or when you need custom AWS configuration.
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            cache: SecretsCache::new(),
        }
    }

    /// Retrieves a secret from AWS Secrets Manager (without caching).
    async fn fetch_from_aws(&self, name: &str) -> Result<SecretValue, SecretsError> {
        debug!("Fetching secret '{}' from AWS Secrets Manager", name);

        let response = self
            .client
            .get_secret_value()
            .secret_id(name)
            .send()
            .await
            .map_err(|e| {
                warn!("Failed to fetch secret '{}' from AWS: {}", name, e);
                SecretsError::BackendError(format!("AWS Secrets Manager error: {}", e))
            })?;

        // AWS secrets can be either strings or binary data
        if let Some(secret_string) = response.secret_string() {
            // Try to parse as JSON first, fall back to plain string
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(secret_string) {
                if json_value.is_object() {
                    debug!("Secret '{}' loaded as JSON object", name);
                    return Ok(SecretValue::Json(json_value));
                }
            }
            debug!("Secret '{}' loaded as string", name);
            Ok(SecretValue::String(secret_string.to_string()))
        } else {
            Err(SecretsError::InvalidFormat(
                "Binary secrets are not supported yet".to_string(),
            ))
        }
    }
}

#[async_trait]
impl SecretsProvider for AwsSecretsProvider {
    async fn get_secret(&self, name: &str) -> Result<SecretValue, SecretsError> {
        // Check cache first
        if let Some(cached_value) = self.cache.get(name).await {
            debug!("Secret '{}' found in cache", name);
            // Try to parse cached value as JSON, fall back to string
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&cached_value) {
                if json_value.is_object() {
                    return Ok(SecretValue::Json(json_value));
                }
            }
            return Ok(SecretValue::String(cached_value));
        }

        // Fetch from AWS and cache the result
        let secret_value = self.fetch_from_aws(name).await?;

        // Cache the raw string representation
        let cache_value = match &secret_value {
            SecretValue::String(s) => s.clone(),
            SecretValue::Json(j) => j.to_string(),
        };
        self.cache.set(name.to_string(), cache_value).await;

        Ok(secret_value)
    }
}
