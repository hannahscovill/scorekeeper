//! JWT authentication middleware with Auth0 JWKS support.

use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::RwLock;

use crate::models::AppError;

/// JWT Claims structure matching Auth0 token format.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Issuer - the Auth0 domain.
    pub iss: String,
    /// Subject - the user ID (e.g., "auth0|123456").
    pub sub: String,
    /// Audience - services this token can access.
    pub aud: Aud,
    /// Issued at timestamp (Unix time).
    pub iat: usize,
    /// Expiration timestamp (Unix time).
    pub exp: usize,
    /// Scopes granted to this token.
    #[serde(default)]
    pub scope: Option<String>,
    /// Authorized party - the client ID.
    #[serde(default)]
    pub azp: Option<String>,
}

/// Audience can be a single string or array of strings.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Aud {
    Single(String),
    Multiple(Vec<String>),
}

impl Aud {
    pub fn contains(&self, audience: &str) -> bool {
        match self {
            Aud::Single(s) => s == audience,
            Aud::Multiple(v) => v.iter().any(|s| s == audience),
        }
    }
}

/// JWKS (JSON Web Key Set) response from Auth0.
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

/// Individual JSON Web Key.
#[derive(Debug, Deserialize, Clone)]
struct Jwk {
    kid: String,
    #[allow(dead_code)]
    kty: String,
    n: String,
    e: String,
}

/// Global JWKS cache.
static JWKS_CACHE: OnceCell<RwLock<HashMap<String, Jwk>>> = OnceCell::new();

fn get_jwks_cache() -> &'static RwLock<HashMap<String, Jwk>> {
    JWKS_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// JWT Authentication handler with JWKS support.
#[derive(Clone)]
pub struct JwtAuth {
    /// Auth0 domain (e.g., "dev-xxx.us.auth0.com")
    auth0_domain: String,
    /// Expected audience
    audience: String,
}

impl JwtAuth {
    /// Creates a new JwtAuth for Auth0.
    pub fn new(auth0_domain: String, audience: String) -> Self {
        Self {
            auth0_domain,
            audience,
        }
    }

    /// Fetches JWKS from Auth0 and caches the keys.
    async fn fetch_jwks(&self) -> Result<(), AppError> {
        let jwks_url = format!("https://{}/.well-known/jwks.json", self.auth0_domain);

        let response = reqwest::get(&jwks_url)
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to fetch JWKS: {}", e)))?;

        let jwks: JwksResponse = response
            .json()
            .await
            .map_err(|e| AppError::InternalError(format!("Failed to parse JWKS: {}", e)))?;

        let cache = get_jwks_cache();
        let mut cache_write = cache
            .write()
            .map_err(|e| AppError::InternalError(format!("JWKS cache lock error: {}", e)))?;

        for key in jwks.keys {
            cache_write.insert(key.kid.clone(), key);
        }

        Ok(())
    }

    /// Gets a key from cache or fetches from Auth0.
    async fn get_key(&self, kid: &str) -> Result<Jwk, AppError> {
        // Try cache first
        {
            let cache = get_jwks_cache();
            if let Ok(cache_read) = cache.read() {
                if let Some(key) = cache_read.get(kid) {
                    return Ok(key.clone());
                }
            }
        }

        // Fetch and retry
        self.fetch_jwks().await?;

        let cache = get_jwks_cache();
        let cache_read = cache
            .read()
            .map_err(|e| AppError::InternalError(format!("JWKS cache lock error: {}", e)))?;

        cache_read
            .get(kid)
            .cloned()
            .ok_or_else(|| AppError::Unauthorized(format!("Unknown key ID: {}", kid)))
    }

    /// Validates a JWT token and returns the claims if valid.
    pub async fn validate_token(&self, token: &str) -> Result<Claims, AppError> {
        // Decode header to get key ID
        let header = decode_header(token)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token header: {}", e)))?;

        let kid = header
            .kid
            .ok_or_else(|| AppError::Unauthorized("Token missing key ID".to_string()))?;

        // Get the signing key
        let jwk = self.get_key(&kid).await?;

        // Create decoding key from RSA components
        let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
            .map_err(|e| AppError::Unauthorized(format!("Invalid key: {}", e)))?;

        // Set up validation
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.audience]);
        validation.set_issuer(&[format!("https://{}/", self.auth0_domain)]);

        // Decode and validate
        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

        Ok(token_data.claims)
    }
}

/// Extracts the bearer token from an HttpRequest.
pub fn extract_bearer_token(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

/// Cookie name for auth token (used by /guess endpoint for embedded games).
pub const AUTH_COOKIE_NAME: &str = "wordle_session";

/// Extracts the auth token from a cookie.
pub fn extract_cookie_token(req: &HttpRequest) -> Option<String> {
    req.cookie(AUTH_COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
}

/// Extracts auth token from either Authorization header (preferred) or cookie.
pub fn extract_token(req: &HttpRequest) -> Option<String> {
    extract_bearer_token(req).or_else(|| extract_cookie_token(req))
}

/// Implement FromRequest for Claims to enable automatic extraction in handlers.
impl FromRequest for Claims {
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();

        Box::pin(async move {
            // Get JwtAuth from app data
            let jwt_auth = req
                .app_data::<web::Data<JwtAuth>>()
                .ok_or_else(|| AppError::InternalError("JwtAuth not configured".to_string()))?;

            // Extract bearer token (Claims extractor requires Authorization header)
            let token = extract_bearer_token(&req).ok_or_else(|| {
                AppError::Unauthorized("Missing Authorization header".to_string())
            })?;

            // Validate and return claims
            jwt_auth.validate_token(&token).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn test_extract_bearer_token() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer my-test-token"))
            .to_http_request();

        let token = extract_bearer_token(&req);
        assert_eq!(token, Some("my-test-token".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        let req = TestRequest::default().to_http_request();

        let token = extract_bearer_token(&req);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_bearer_token_invalid_prefix() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Basic my-test-token"))
            .to_http_request();

        let token = extract_bearer_token(&req);
        assert!(token.is_none());
    }

    #[test]
    fn test_aud_single_contains() {
        let aud = Aud::Single("my-audience".to_string());
        assert!(aud.contains("my-audience"));
        assert!(!aud.contains("other"));
    }

    #[test]
    fn test_aud_multiple_contains() {
        let aud = Aud::Multiple(vec!["aud1".to_string(), "aud2".to_string()]);
        assert!(aud.contains("aud1"));
        assert!(aud.contains("aud2"));
        assert!(!aud.contains("aud3"));
    }

    #[test]
    fn test_extract_cookie_token() {
        let req = TestRequest::default()
            .cookie(actix_web::cookie::Cookie::new(
                "wordle_session",
                "cookie-token",
            ))
            .to_http_request();

        let token = extract_cookie_token(&req);
        assert_eq!(token, Some("cookie-token".to_string()));
    }

    #[test]
    fn test_extract_cookie_token_missing() {
        let req = TestRequest::default().to_http_request();

        let token = extract_cookie_token(&req);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_token_prefers_bearer() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer bearer-token"))
            .cookie(actix_web::cookie::Cookie::new(
                "wordle_session",
                "cookie-token",
            ))
            .to_http_request();

        let token = extract_token(&req);
        assert_eq!(token, Some("bearer-token".to_string()));
    }

    #[test]
    fn test_extract_token_falls_back_to_cookie() {
        let req = TestRequest::default()
            .cookie(actix_web::cookie::Cookie::new(
                "wordle_session",
                "cookie-token",
            ))
            .to_http_request();

        let token = extract_token(&req);
        assert_eq!(token, Some("cookie-token".to_string()));
    }
}
