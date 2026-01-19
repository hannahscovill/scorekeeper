//! JWT authentication middleware.

use actix_web::{dev::ServiceRequest, HttpMessage, HttpRequest};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tracing;
use uuid::Uuid;

use crate::models::AppError;

/// JWT Claims structure for authentication tokens.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject - the user ID.
    pub sub: Uuid,
    /// Expiration timestamp (Unix time).
    pub exp: usize,
    /// Issued at timestamp (Unix time).
    pub iat: usize,
    /// Optional team ID for team-scoped access.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<Uuid>,
}

/// JWT Authentication handler.
#[derive(Clone)]
pub struct JwtAuth {
    secret: String,
}

impl JwtAuth {
    /// Creates a new JwtAuth with the given secret.
    pub fn new(secret: String) -> Self {
        Self { secret }
    }

    /// Validates a JWT token and returns the claims if valid.
    pub fn validate_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let validation = Validation::new(Algorithm::HS256);
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )?;
        Ok(token_data.claims)
    }
}

/// Extracts the bearer token from the Authorization header.
pub fn extract_bearer_token(req: &ServiceRequest) -> Option<String> {
    req.headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

/// Extracts the bearer token from an HttpRequest.
pub fn extract_bearer_token_from_request(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}

/// Validates a JWT token from the request and returns the claims.
///
/// This function extracts the bearer token from the Authorization header,
/// validates it using the provided JwtAuth, and stores the claims in the
/// request extensions for later access by handlers.
pub fn validate_jwt_from_request(
    req: &ServiceRequest,
    jwt_auth: &JwtAuth,
) -> Result<Claims, AppError> {
    let token = extract_bearer_token(req)
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

    let claims = jwt_auth
        .validate_token(&token)
        .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

    // Store claims in request extensions for handlers to access
    req.extensions_mut().insert(claims.clone());

    Ok(claims)
}

/// Creates mock claims for local development when auth bypass is enabled.
///
/// # Security Warning
/// This function should ONLY be used when BYPASS_AUTH=true in development.
/// Never use in production environments.
pub fn dev_claims() -> Claims {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Well-known dev user ID (consistent across sessions)
    let dev_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    Claims {
        sub: dev_user_id,
        exp: now + 86400 * 365, // 1 year from now (effectively never expires for dev)
        iat: now,
        team_id: None, // No team by default in dev mode
    }
}

/// Validates a JWT token from the request, with optional bypass for development.
///
/// # Security Warning
/// When bypass_auth is true, this skips all JWT validation and returns mock claims.
/// This should ONLY be enabled in local development environments.
pub fn validate_jwt_from_request_with_bypass(
    req: &ServiceRequest,
    jwt_auth: &JwtAuth,
    bypass_auth: bool,
) -> Result<Claims, AppError> {
    if bypass_auth {
        // SECURITY: Auth bypass is enabled - using mock claims
        tracing::warn!("⚠️  AUTH BYPASS ENABLED - Using dev claims instead of JWT validation");
        let claims = dev_claims();
        req.extensions_mut().insert(claims.clone());
        return Ok(claims);
    }

    // Normal JWT validation path
    validate_jwt_from_request(req, jwt_auth)
}

/// Validates an API key from the request headers.
///
/// This is a placeholder implementation that will be expanded later.
pub fn validate_api_key(req: &HttpRequest) -> Result<(), AppError> {
    // Placeholder: accept any request for now
    let _ = req;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_test_jwt_auth() -> JwtAuth {
        JwtAuth::new("test-secret-key-for-jwt-testing".to_string())
    }

    fn get_current_timestamp() -> usize {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
    }

    fn create_test_token(claims: &Claims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_valid_token_validation() {
        let jwt_auth = create_test_jwt_auth();
        let now = get_current_timestamp();
        let claims = Claims {
            sub: Uuid::new_v4(),
            exp: now + 3600, // 1 hour from now
            iat: now,
            team_id: Some(Uuid::new_v4()),
        };

        let token = create_test_token(&claims, "test-secret-key-for-jwt-testing");
        let result = jwt_auth.validate_token(&token);

        assert!(result.is_ok());
        let validated_claims = result.unwrap();
        assert_eq!(validated_claims.sub, claims.sub);
        assert_eq!(validated_claims.team_id, claims.team_id);
    }

    #[test]
    fn test_expired_token_rejection() {
        let jwt_auth = create_test_jwt_auth();
        let now = get_current_timestamp();
        let claims = Claims {
            sub: Uuid::new_v4(),
            exp: now - 3600, // 1 hour ago (expired)
            iat: now - 7200,
            team_id: None,
        };

        let token = create_test_token(&claims, "test-secret-key-for-jwt-testing");
        let result = jwt_auth.validate_token(&token);

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_token_rejection() {
        let jwt_auth = create_test_jwt_auth();
        let now = get_current_timestamp();
        let claims = Claims {
            sub: Uuid::new_v4(),
            exp: now + 3600,
            iat: now,
            team_id: None,
        };

        // Create token with different secret
        let token = create_test_token(&claims, "wrong-secret-key");
        let result = jwt_auth.validate_token(&token);

        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_token_rejection() {
        let jwt_auth = create_test_jwt_auth();
        let result = jwt_auth.validate_token("not-a-valid-jwt-token");

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_bearer_token() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer my-test-token"))
            .to_srv_request();

        let token = extract_bearer_token(&req);
        assert_eq!(token, Some("my-test-token".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        let req = TestRequest::default().to_srv_request();

        let token = extract_bearer_token(&req);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_bearer_token_invalid_prefix() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Basic my-test-token"))
            .to_srv_request();

        let token = extract_bearer_token(&req);
        assert!(token.is_none());
    }

    #[test]
    fn test_validate_api_key_placeholder() {
        let req = TestRequest::default().to_http_request();
        assert!(validate_api_key(&req).is_ok());
    }

    #[test]
    fn test_claims_without_team_id() {
        let jwt_auth = create_test_jwt_auth();
        let now = get_current_timestamp();
        let claims = Claims {
            sub: Uuid::new_v4(),
            exp: now + 3600,
            iat: now,
            team_id: None,
        };

        let token = create_test_token(&claims, "test-secret-key-for-jwt-testing");
        let result = jwt_auth.validate_token(&token);

        assert!(result.is_ok());
        let validated_claims = result.unwrap();
        assert!(validated_claims.team_id.is_none());
    }
}
