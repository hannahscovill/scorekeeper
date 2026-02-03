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
    /// Auth0 M2M client ID for Management API access.
    pub auth0_m2m_client_id: Option<String>,
    /// Auth0 M2M client secret for Management API access.
    pub auth0_m2m_client_secret: Option<String>,
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
    /// Comma-separated list of allowed CORS origins.
    pub cors_allowed_origins: Vec<String>,
    /// S3 bucket name for avatar uploads.
    pub s3_avatar_bucket: Option<String>,
    /// S3 bucket name for common words file (puzzle word selection).
    pub s3_common_words_bucket: Option<String>,
    /// S3 key for common words file (defaults to "common_words.txt").
    pub s3_common_words_key: Option<String>,
    /// Comma-separated list of admin user IDs (Auth0 subjects).
    pub admin_user_ids: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database_url: None,
            auth0_domain: "dev-g32naui5mvpwnsg7.us.auth0.com".to_string(),
            auth0_audience: "com.hannahscovill.scorekeeper".to_string(),
            auth0_m2m_client_id: None,
            auth0_m2m_client_secret: None,
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            dynamodb_endpoint_url: None,
            dynamodb_table_name: None,
            cors_allowed_origins: vec![
                "http://localhost:3000".to_string(),
                "https://localhost:3000".to_string(),
                "https://d3g0psl1n9xo36.cloudfront.net".to_string(),
                "https://wordles.dev".to_string(),
                "https://hannahscovill.github.io".to_string(),
            ],
            s3_avatar_bucket: None,
            s3_common_words_bucket: None,
            s3_common_words_key: None,
            admin_user_ids: Vec::new(),
        }
    }
}

impl Config {
    /// Creates a new configuration from environment variables.
    pub fn from_env() -> Self {
        let default_origins = vec![
            "http://localhost:3000".to_string(),
            "https://localhost:3000".to_string(),
            "https://d3g0psl1n9xo36.cloudfront.net".to_string(),
            "https://wordles.dev".to_string(),
            "https://hannahscovill.github.io".to_string(),
        ];

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
            auth0_m2m_client_id: std::env::var("AUTH0_M2M_CLIENT_ID").ok(),
            auth0_m2m_client_secret: std::env::var("AUTH0_M2M_CLIENT_SECRET").ok(),
            tls_enabled: std::env::var("TLS_ENABLED")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            tls_cert_path: std::env::var("TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("TLS_KEY_PATH").ok(),
            dynamodb_endpoint_url: std::env::var("AWS_ENDPOINT_URL_DYNAMODB").ok(),
            dynamodb_table_name: std::env::var("DYNAMODB_TABLE").ok(),
            cors_allowed_origins: std::env::var("CORS_ALLOWED_ORIGINS")
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or(default_origins),
            s3_avatar_bucket: std::env::var("S3_AVATAR_BUCKET").ok(),
            s3_common_words_bucket: std::env::var("S3_COMMON_WORDS_BUCKET").ok(),
            s3_common_words_key: std::env::var("S3_COMMON_WORDS_KEY").ok(),
            admin_user_ids: std::env::var("ADMIN_USER_IDS")
                .map(|s| {
                    s.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
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

    /// Returns the Auth0 M2M client ID.
    pub fn auth0_m2m_client_id(&self) -> Option<&str> {
        self.auth0_m2m_client_id.as_deref()
    }

    /// Returns the Auth0 M2M client secret.
    pub fn auth0_m2m_client_secret(&self) -> Option<&str> {
        self.auth0_m2m_client_secret.as_deref()
    }

    /// Returns the allowed CORS origins.
    pub fn cors_allowed_origins(&self) -> &[String] {
        &self.cors_allowed_origins
    }

    /// Returns the S3 avatar bucket name.
    pub fn s3_avatar_bucket(&self) -> Option<&str> {
        self.s3_avatar_bucket.as_deref()
    }

    /// Returns the S3 bucket name for common words.
    pub fn s3_common_words_bucket(&self) -> Option<&str> {
        self.s3_common_words_bucket.as_deref()
    }

    /// Returns the S3 key for common words (defaults to "common_words.txt").
    pub fn s3_common_words_key(&self) -> &str {
        self.s3_common_words_key
            .as_deref()
            .unwrap_or("common_words.txt")
    }

    /// Returns the list of admin user IDs.
    pub fn admin_user_ids(&self) -> &[String] {
        &self.admin_user_ids
    }

    /// Checks if a user ID is an admin.
    pub fn is_admin(&self, user_id: &str) -> bool {
        self.admin_user_ids.iter().any(|id| id == user_id)
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
        assert!(config.auth0_m2m_client_id.is_none());
        assert!(config.auth0_m2m_client_secret.is_none());
        assert!(!config.tls_enabled);
        assert!(config.tls_cert_path.is_none());
        assert!(config.tls_key_path.is_none());
        assert!(config.dynamodb_endpoint_url.is_none());
        assert!(config.dynamodb_table_name.is_none());
        assert_eq!(config.cors_allowed_origins.len(), 5);
        assert!(config
            .cors_allowed_origins
            .contains(&"http://localhost:3000".to_string()));
        assert!(config
            .cors_allowed_origins
            .contains(&"https://localhost:3000".to_string()));
        assert!(config
            .cors_allowed_origins
            .contains(&"https://d3g0psl1n9xo36.cloudfront.net".to_string()));
        assert!(config
            .cors_allowed_origins
            .contains(&"https://wordles.dev".to_string()));
        assert!(config
            .cors_allowed_origins
            .contains(&"https://hannahscovill.github.io".to_string()));
        assert!(config.s3_avatar_bucket.is_none());
        assert!(config.s3_common_words_bucket.is_none());
        assert!(config.s3_common_words_key.is_none());
        assert!(config.admin_user_ids.is_empty());
    }

    #[test]
    fn test_is_admin() {
        let mut config = Config::default();
        config.admin_user_ids = vec!["auth0|admin1".to_string(), "auth0|admin2".to_string()];

        assert!(config.is_admin("auth0|admin1"));
        assert!(config.is_admin("auth0|admin2"));
        assert!(!config.is_admin("auth0|user1"));
        assert!(!config.is_admin(""));
    }
}
