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
    /// JWT secret for token signing and validation.
    pub jwt_secret: String,
    /// Bypass authentication (for development/testing only).
    pub bypass_auth: bool,
    /// Enable TLS/HTTPS.
    pub tls_enabled: bool,
    /// Path to TLS certificate file (.pem).
    pub tls_cert_path: Option<String>,
    /// Path to TLS private key file (.pem).
    pub tls_key_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database_url: None,
            jwt_secret: "development-secret-change-in-production".to_string(),
            bypass_auth: false,
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
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
            bypass_auth: std::env::var("BYPASS_AUTH")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            tls_enabled: std::env::var("TLS_ENABLED")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            tls_cert_path: std::env::var("TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("TLS_KEY_PATH").ok(),
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

    /// Returns whether authentication should be bypassed.
    pub fn bypass_auth(&self) -> bool {
        self.bypass_auth
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
        assert_eq!(config.bypass_auth, false);
        assert_eq!(config.tls_enabled, false);
        assert!(config.tls_cert_path.is_none());
        assert!(config.tls_key_path.is_none());
    }
}
