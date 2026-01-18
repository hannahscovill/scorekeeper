//! Environment variable secrets provider implementation.

use async_trait::async_trait;
use tracing::debug;

use super::provider::{SecretValue, SecretsError, SecretsProvider};

/// Secrets provider that loads secrets from environment variables.
///
/// This provider is useful for local development and testing where
/// AWS Secrets Manager may not be available or desirable.
///
/// Environment variable names are converted by replacing forward slashes
/// and hyphens with underscores and converting to uppercase.
/// For example: "prod/db/password" becomes "PROD_DB_PASSWORD"
pub struct EnvSecretsProvider;

impl EnvSecretsProvider {
    /// Creates a new environment variable secrets provider.
    pub fn new() -> Self {
        Self
    }

    /// Converts a secret name to an environment variable name.
    ///
    /// Examples:
    /// - "JWT_SECRET" -> "JWT_SECRET"
    /// - "prod/db/password" -> "PROD_DB_PASSWORD"
    /// - "app-config" -> "APP_CONFIG"
    fn to_env_var_name(name: &str) -> String {
        name.replace(['/', '-'], "_").to_uppercase()
    }
}

impl Default for EnvSecretsProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretsProvider for EnvSecretsProvider {
    async fn get_secret(&self, name: &str) -> Result<SecretValue, SecretsError> {
        let env_var_name = Self::to_env_var_name(name);
        debug!(
            "Looking for secret '{}' in environment variable '{}'",
            name, env_var_name
        );

        let value = std::env::var(&env_var_name).map_err(|_| {
            SecretsError::NotFound(format!("Environment variable '{}' not set", env_var_name))
        })?;

        // Try to parse as JSON first
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&value) {
            if json_value.is_object() {
                debug!(
                    "Environment variable '{}' parsed as JSON object",
                    env_var_name
                );
                return Ok(SecretValue::Json(json_value));
            }
        }

        debug!("Environment variable '{}' loaded as string", env_var_name);
        Ok(SecretValue::String(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_name_conversion() {
        assert_eq!(
            EnvSecretsProvider::to_env_var_name("JWT_SECRET"),
            "JWT_SECRET"
        );
        assert_eq!(
            EnvSecretsProvider::to_env_var_name("prod/db/password"),
            "PROD_DB_PASSWORD"
        );
        assert_eq!(
            EnvSecretsProvider::to_env_var_name("app-config"),
            "APP_CONFIG"
        );
        assert_eq!(
            EnvSecretsProvider::to_env_var_name("my-app/prod/api-key"),
            "MY_APP_PROD_API_KEY"
        );
    }

    #[tokio::test]
    async fn test_get_existing_env_var() {
        std::env::set_var("TEST_SECRET_KEY", "test-value-123");

        let provider = EnvSecretsProvider::new();
        let result = provider.get_secret("TEST_SECRET_KEY").await;

        assert!(result.is_ok());
        if let Ok(SecretValue::String(value)) = result {
            assert_eq!(value, "test-value-123");
        } else {
            panic!("Expected String secret value");
        }

        std::env::remove_var("TEST_SECRET_KEY");
    }

    #[tokio::test]
    async fn test_get_missing_env_var() {
        let provider = EnvSecretsProvider::new();
        let result = provider.get_secret("NONEXISTENT_SECRET_XYZ").await;

        assert!(result.is_err());
        match result {
            Err(SecretsError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }
}
