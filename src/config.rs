//! Configuration management for the scorekeeper server.

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
    /// Auth0 domain (e.g., "dev-xxx.us.auth0.com").
    pub auth0_domain: String,
    /// Auth0 audience (API identifier).
    pub auth0_audience: String,
    /// Enable TLS/HTTPS.
    pub tls_enabled: bool,
    /// Path to TLS certificate file (.pem).
    pub tls_cert_path: Option<String>,
    /// Path to TLS private key file (.pem).
    pub tls_key_path: Option<String>,
    /// DynamoDB endpoint URL (for local development).
    pub dynamodb_endpoint_url: Option<String>,
    /// DynamoDB table name.
    pub dynamodb_table_name: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database_url: None,
            auth0_domain: "dev-g32naui5mvpwnsg7.us.auth0.com".to_string(),
            auth0_audience: "com.hannahscovill.scorekeeper".to_string(),
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            dynamodb_endpoint_url: None,
            dynamodb_table_name: None,
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
            auth0_domain: std::env::var("AUTH0_DOMAIN")
                .unwrap_or_else(|_| "dev-g32naui5mvpwnsg7.us.auth0.com".to_string()),
            auth0_audience: std::env::var("AUTH0_AUDIENCE")
                .unwrap_or_else(|_| "com.hannahscovill.scorekeeper".to_string()),
            tls_enabled: std::env::var("TLS_ENABLED")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            tls_cert_path: std::env::var("TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("TLS_KEY_PATH").ok(),
            dynamodb_endpoint_url: std::env::var("AWS_ENDPOINT_URL_DYNAMODB").ok(),
            dynamodb_table_name: std::env::var("DYNAMODB_TABLE_NAME").ok(),
        }
    }

    /// Returns the bind address as a tuple.
    pub fn bind_address(&self) -> (String, u16) {
        (self.host.clone(), self.port)
    }

    /// Returns the Auth0 domain.
    pub fn auth0_domain(&self) -> &str {
        &self.auth0_domain
    }

    /// Returns the Auth0 audience.
    pub fn auth0_audience(&self) -> &str {
        &self.auth0_audience
    }

    /// Returns whether TLS is enabled.
    pub fn tls_enabled(&self) -> bool {
        self.tls_enabled
    }

    /// Returns the TLS certificate path.
    pub fn tls_cert_path(&self) -> Option<&str> {
        self.tls_cert_path.as_deref()
    }

    /// Returns the TLS private key path.
    pub fn tls_key_path(&self) -> Option<&str> {
        self.tls_key_path.as_deref()
    }

    /// Returns the DynamoDB endpoint URL.
    pub fn dynamodb_endpoint_url(&self) -> Option<&str> {
        self.dynamodb_endpoint_url.as_deref()
    }

    /// Returns the DynamoDB table name.
    pub fn dynamodb_table_name(&self) -> Option<&str> {
        self.dynamodb_table_name.as_deref()
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
        assert_eq!(config.auth0_domain, "dev-g32naui5mvpwnsg7.us.auth0.com");
        assert_eq!(config.auth0_audience, "com.hannahscovill.scorekeeper");
        assert_eq!(config.tls_enabled, false);
        assert!(config.tls_cert_path.is_none());
        assert!(config.tls_key_path.is_none());
        assert!(config.dynamodb_endpoint_url.is_none());
        assert!(config.dynamodb_table_name.is_none());
    }
}
