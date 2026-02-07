//! Configuration management for the scorekeeper server.

use serde::Deserialize;

/// Deployment environment — parsed once from the `ENVIRONMENT` env var.
#[derive(Debug, Clone, PartialEq)]
pub enum Environment {
    /// Local development (docker-compose, cargo run, npm run dev).
    Local,
    /// AWS production deployment.
    Production,
}

impl Environment {
    const LOCAL: &str = "local";
    const PRODUCTION: &str = "production";

    /// Parse from the `ENVIRONMENT` env var. Defaults to `Local`.
    pub fn from_env() -> Self {
        match std::env::var("ENVIRONMENT").as_deref() {
            Ok(Self::PRODUCTION) => Self::Production,
            _ => Self::Local,
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => Self::LOCAL,
            Self::Production => Self::PRODUCTION,
        }
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
    /// Local file path for common words (checked first, use for local dev).
    pub common_words_file_path: Option<String>,
    /// Comma-separated list of admin user IDs (Auth0 subjects).
    pub admin_user_ids: Vec<String>,
    /// GitHub App ID for issue creation.
    pub github_app_id: Option<String>,
    /// GitHub App installation ID.
    pub github_installation_id: Option<String>,
    /// GitHub App private key (PEM format, from env var or file).
    pub github_private_key: Option<String>,
    /// GitHub personal access token (for local dev).
    pub github_token: Option<String>,
    /// GitHub repository (owner/repo) to create issues in.
    pub github_repo: String,
    /// Cloudflare Turnstile secret key for CAPTCHA verification.
    pub turnstile_secret_key: Option<String>,
    /// Cloudflare Turnstile verification URL.
    pub turnstile_verify_url: String,
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
            common_words_file_path: None,
            admin_user_ids: Vec::new(),
            github_app_id: None,
            github_installation_id: None,
            github_private_key: None,
            github_token: None,
            github_repo: "hannahscovill/wordles-with-friends-client-web".to_string(),
            turnstile_secret_key: None,
            turnstile_verify_url: "https://challenges.cloudflare.com/turnstile/v0/siteverify"
                .to_string(),
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
            dynamodb_table_name: std::env::var("DYNAMODB_TABLE_NAME").ok(),
            cors_allowed_origins: std::env::var("CORS_ALLOWED_ORIGINS")
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or(default_origins),
            s3_avatar_bucket: std::env::var("S3_AVATAR_BUCKET").ok(),
            s3_common_words_bucket: std::env::var("S3_COMMON_WORDS_BUCKET").ok(),
            s3_common_words_key: std::env::var("S3_COMMON_WORDS_KEY").ok(),
            common_words_file_path: std::env::var("COMMON_WORDS_FILE_PATH").ok(),
            admin_user_ids: std::env::var("ADMIN_USER_IDS")
                .map(|s| {
                    s.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            github_app_id: std::env::var("GITHUB_APP_ID").ok().filter(|s| !s.is_empty()),
            github_installation_id: std::env::var("GITHUB_INSTALLATION_ID").ok().filter(|s| !s.is_empty()),
            github_private_key: std::env::var("GITHUB_PRIVATE_KEY")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    std::env::var("GITHUB_PRIVATE_KEY_FILE")
                        .ok()
                        .filter(|s| !s.is_empty())
                        .and_then(|path| std::fs::read_to_string(&path).ok())
                }),
            github_token: std::env::var("GITHUB_TOKEN").ok().filter(|s| !s.is_empty()),
            github_repo: std::env::var("GITHUB_REPO").unwrap_or_else(|_| {
                "hannahscovill/wordles-with-friends-client-web".to_string()
            }),
            turnstile_secret_key: std::env::var("TURNSTILE_SECRET_KEY").ok().filter(|s| !s.is_empty()),
            turnstile_verify_url: std::env::var("TURNSTILE_VERIFY_URL").unwrap_or_else(|_| {
                "https://challenges.cloudflare.com/turnstile/v0/siteverify".to_string()
            }),
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

    /// Returns the local file path for common words (for local dev).
    pub fn common_words_file_path(&self) -> Option<&str> {
        self.common_words_file_path.as_deref()
    }

    /// Returns the list of admin user IDs.
    pub fn admin_user_ids(&self) -> &[String] {
        &self.admin_user_ids
    }

    /// Checks if a user ID is an admin.
    pub fn is_admin(&self, user_id: &str) -> bool {
        self.admin_user_ids.iter().any(|id| id == user_id)
    }

    /// Returns the GitHub App ID.
    pub fn github_app_id(&self) -> Option<&str> {
        self.github_app_id.as_deref()
    }

    /// Returns the GitHub App installation ID.
    pub fn github_installation_id(&self) -> Option<&str> {
        self.github_installation_id.as_deref()
    }

    /// Returns the GitHub App private key.
    pub fn github_private_key(&self) -> Option<&str> {
        self.github_private_key.as_deref()
    }

    /// Returns the GitHub personal access token.
    pub fn github_token(&self) -> Option<&str> {
        self.github_token.as_deref()
    }

    /// Returns the GitHub repository (owner/repo).
    pub fn github_repo(&self) -> &str {
        &self.github_repo
    }

    /// Returns the Turnstile secret key.
    pub fn turnstile_secret_key(&self) -> Option<&str> {
        self.turnstile_secret_key.as_deref()
    }

    /// Returns the Turnstile verification URL.
    pub fn turnstile_verify_url(&self) -> &str {
        &self.turnstile_verify_url
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
        assert!(config.common_words_file_path.is_none());
        assert!(config.admin_user_ids.is_empty());
        assert!(config.github_app_id.is_none());
        assert!(config.github_installation_id.is_none());
        assert!(config.github_private_key.is_none());
        assert!(config.github_token.is_none());
        assert_eq!(
            config.github_repo,
            "hannahscovill/wordles-with-friends-client-web"
        );
        assert!(config.turnstile_secret_key.is_none());
        assert_eq!(
            config.turnstile_verify_url,
            "https://challenges.cloudflare.com/turnstile/v0/siteverify"
        );
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
