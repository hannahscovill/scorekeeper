//! Configuration management for the scorekeeper server.

use crate::secrets::{SecretsError, SecretsProvider};
use serde::Deserialize;

/// Server configuration settings.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Host address to bind to.
    pub host: String,
    /// Port to listen on.
    pub port: u16,
    /// Database URL.
    pub database_url: Option<String>,
    /// JWT secret for token signing and validation.
    pub jwt_secret: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database_url: None,
            jwt_secret: "development-secret-change-in-production".to_string(),
        }
    }
}

impl Config {
    /// Creates a new configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            database_url: std::env::var("DATABASE_URL").ok(),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "development-secret-change-in-production".to_string()),
        }
    }

    /// Returns the bind address as a tuple.
    pub fn bind_address(&self) -> (String, u16) {
        (self.host.clone(), self.port)
    }

    /// Returns the JWT secret.
    pub fn jwt_secret(&self) -> &str {
        &self.jwt_secret
    }

    /// Creates a new configuration using a secrets provider.
    ///
    /// This method loads secrets from the provided secrets provider (e.g., AWS Secrets Manager)
    /// with fallback to environment variables for non-sensitive configuration.
    ///
    /// # Arguments
    /// * `secrets_provider` - The provider to use for loading secrets
    /// * `secret_name` - The name of the secret containing sensitive configuration (optional)
    ///
    /// # Returns
    /// A Result containing the configuration or an error if secrets couldn't be loaded.
    pub async fn from_secrets(
        secrets_provider: &dyn SecretsProvider,
        secret_name: Option<&str>,
    ) -> Result<Self, SecretsError> {
        // Load non-sensitive config from environment
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        // Load sensitive config from secrets provider
        let jwt_secret = if let Some(name) = secret_name {
            // Try to get from the specified secret
            match secrets_provider.get_secret(name).await {
                Ok(secret_value) => {
                    // If it's a JSON object, look for "jwt_secret" key
                    secret_value
                        .as_string(Some("jwt_secret"))
                        .unwrap_or_else(|_| {
                            // If not found, fall back to environment variable
                            std::env::var("JWT_SECRET").unwrap_or_else(|_| {
                                "development-secret-change-in-production".to_string()
                            })
                        })
                }
                Err(_) => {
                    // If secret not found, fall back to environment variable
                    std::env::var("JWT_SECRET")
                        .unwrap_or_else(|_| "development-secret-change-in-production".to_string())
                }
            }
        } else {
            // Try individual secret names
            match secrets_provider.get_secret("JWT_SECRET").await {
                Ok(secret_value) => secret_value.as_string(None).unwrap_or_else(|_| {
                    std::env::var("JWT_SECRET")
                        .unwrap_or_else(|_| "development-secret-change-in-production".to_string())
                }),
                Err(_) => std::env::var("JWT_SECRET")
                    .unwrap_or_else(|_| "development-secret-change-in-production".to_string()),
            }
        };

        let database_url = if let Some(name) = secret_name {
            match secrets_provider.get_secret(name).await {
                Ok(secret_value) => secret_value.as_string(Some("database_url")).ok(),
                Err(_) => std::env::var("DATABASE_URL").ok(),
            }
        } else {
            match secrets_provider.get_secret("DATABASE_URL").await {
                Ok(secret_value) => secret_value.as_string(None).ok(),
                Err(_) => std::env::var("DATABASE_URL").ok(),
            }
        };

        Ok(Self {
            host,
            port,
            database_url,
            jwt_secret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.database_url.is_none());
        assert_eq!(config.jwt_secret, "development-secret-change-in-production");
    }
}
