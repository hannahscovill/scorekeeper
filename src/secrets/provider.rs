//! Secrets provider trait and common types.

use async_trait::async_trait;
use thiserror::Error;

/// Errors that can occur when loading secrets.
#[derive(Debug, Error)]
pub enum SecretsError {
    /// The requested secret was not found.
    #[error("Secret not found: {0}")]
    NotFound(String),

    /// An error occurred while accessing the secrets backend.
    #[error("Failed to access secrets backend: {0}")]
    BackendError(String),

    /// The secret value could not be parsed or decoded.
    #[error("Invalid secret format: {0}")]
    InvalidFormat(String),
}

/// A secret value that can be either a simple string or a JSON object.
#[derive(Debug, Clone)]
pub enum SecretValue {
    /// A simple string value.
    String(String),
    /// A JSON object with multiple key-value pairs.
    Json(serde_json::Value),
}

impl SecretValue {
    /// Extracts a string value from the secret.
    ///
    /// For String variants, returns the string directly.
    /// For Json variants, attempts to extract the value at the given key.
    pub fn as_string(&self, key: Option<&str>) -> Result<String, SecretsError> {
        match self {
            SecretValue::String(s) => Ok(s.clone()),
            SecretValue::Json(obj) => {
                let key = key.ok_or_else(|| {
                    SecretsError::InvalidFormat(
                        "JSON secret requires a key to extract value".to_string(),
                    )
                })?;
                obj.get(key)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        SecretsError::InvalidFormat(format!(
                            "Key '{}' not found in JSON secret",
                            key
                        ))
                    })
            }
        }
    }
}

/// Trait for providing secrets from various sources.
///
/// Implementors can load secrets from AWS Secrets Manager, environment variables,
/// local files, or any other source. The trait is async to support remote backends.
#[async_trait]
pub trait SecretsProvider: Send + Sync {
    /// Retrieves a secret by name.
    ///
    /// # Arguments
    /// * `name` - The name/identifier of the secret to retrieve
    ///
    /// # Returns
    /// The secret value if found, or an error if the secret doesn't exist or
    /// couldn't be accessed.
    async fn get_secret(&self, name: &str) -> Result<SecretValue, SecretsError>;

    /// Checks if this provider can handle the given secret name.
    ///
    /// This allows for provider chaining and fallback logic.
    fn can_provide(&self, name: &str) -> bool {
        // By default, all providers can attempt to provide any secret
        let _ = name;
        true
    }
}
