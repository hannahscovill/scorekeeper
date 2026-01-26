//! Auth0 Management API client service.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{debug, error};

use crate::models::error::AppError;

/// Cached M2M access token with expiration.
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

/// Auth0 Management API service for managing users.
pub struct Auth0ManagementService {
    client: Client,
    domain: String,
    client_id: String,
    client_secret: String,
    token_cache: RwLock<Option<CachedToken>>,
}

/// Response from Auth0 token endpoint.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

/// Request body for updating user metadata.
#[derive(Debug, Serialize)]
struct UpdateUserRequest {
    user_metadata: UserMetadataUpdate,
}

/// User metadata for updates (all fields optional).
#[derive(Debug, Serialize)]
struct UserMetadataUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
}

/// User metadata from Auth0.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserMetadata {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// Response from Auth0 user endpoints.
#[derive(Debug, Deserialize)]
pub struct Auth0User {
    pub user_id: String,
    #[serde(default)]
    pub user_metadata: Option<UserMetadata>,
}

impl Auth0ManagementService {
    /// Creates a new Auth0ManagementService.
    pub fn new(domain: String, client_id: String, client_secret: String) -> Self {
        Self {
            client: Client::new(),
            domain,
            client_id,
            client_secret,
            token_cache: RwLock::new(None),
        }
    }

    /// Gets a valid M2M access token, fetching a new one if the cached token is expired.
    async fn get_access_token(&self) -> Result<String, AppError> {
        // Check if we have a valid cached token
        {
            let cache = self.token_cache.read().map_err(|e| {
                error!("Failed to acquire read lock on token cache: {}", e);
                AppError::internal("Token cache lock error")
            })?;

            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > Instant::now() {
                    debug!("Using cached M2M token");
                    return Ok(cached.access_token.clone());
                }
            }
        }

        // Fetch a new token
        debug!("Fetching new M2M token from Auth0");
        let token_url = format!("https://{}/oauth/token", self.domain);

        let response = self
            .client
            .post(&token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("audience", &format!("https://{}/api/v2/", self.domain)),
            ])
            .send()
            .await
            .map_err(|e| {
                error!("Failed to request M2M token: {}", e);
                AppError::internal("Failed to communicate with Auth0")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Auth0 token request failed: {} - {}", status, body);
            return Err(AppError::internal("Failed to obtain Auth0 access token"));
        }

        let token_response: TokenResponse = response.json().await.map_err(|e| {
            error!("Failed to parse token response: {}", e);
            AppError::internal("Invalid token response from Auth0")
        })?;

        // Cache the token with a buffer before expiration (subtract 60 seconds)
        let expires_at =
            Instant::now() + Duration::from_secs(token_response.expires_in.saturating_sub(60));

        let access_token = token_response.access_token.clone();

        {
            let mut cache = self.token_cache.write().map_err(|e| {
                error!("Failed to acquire write lock on token cache: {}", e);
                AppError::internal("Token cache lock error")
            })?;

            *cache = Some(CachedToken {
                access_token: token_response.access_token,
                expires_at,
            });
        }

        debug!("Successfully obtained and cached new M2M token");
        Ok(access_token)
    }

    /// Gets a user from Auth0 by user ID.
    pub async fn get_user(&self, user_id: &str) -> Result<Auth0User, AppError> {
        let token = self.get_access_token().await?;

        let url = format!(
            "https://{}/api/v2/users/{}",
            self.domain,
            urlencoding::encode(user_id)
        );

        debug!("Fetching user from Auth0: {}", user_id);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to fetch user from Auth0: {}", e);
                AppError::internal("Failed to communicate with Auth0")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Auth0 get user failed: {} - {}", status, body);

            return match status.as_u16() {
                404 => Err(AppError::not_found("User not found")),
                401 | 403 => Err(AppError::internal(
                    "Auth0 Management API authorization failed",
                )),
                _ => Err(AppError::internal("Failed to fetch user from Auth0")),
            };
        }

        let user: Auth0User = response.json().await.map_err(|e| {
            error!("Failed to parse user response: {}", e);
            AppError::internal("Invalid response from Auth0")
        })?;

        Ok(user)
    }

    /// Updates a user's metadata in Auth0.
    pub async fn update_user_metadata(
        &self,
        user_id: &str,
        display_name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<Auth0User, AppError> {
        let token = self.get_access_token().await?;

        let url = format!(
            "https://{}/api/v2/users/{}",
            self.domain,
            urlencoding::encode(user_id)
        );

        let request_body = UpdateUserRequest {
            user_metadata: UserMetadataUpdate {
                display_name: display_name.map(|s| s.to_string()),
                avatar_url: avatar_url.map(|s| s.to_string()),
            },
        };

        debug!("Updating user metadata for user: {}", user_id);

        let response = self
            .client
            .patch(&url)
            .bearer_auth(&token)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to update user in Auth0: {}", e);
                AppError::internal("Failed to communicate with Auth0")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Auth0 user update failed: {} - {}", status, body);

            return match status.as_u16() {
                404 => Err(AppError::not_found("User not found")),
                401 | 403 => Err(AppError::internal(
                    "Auth0 Management API authorization failed",
                )),
                _ => Err(AppError::internal("Failed to update user in Auth0")),
            };
        }

        let user: Auth0User = response.json().await.map_err(|e| {
            error!("Failed to parse user update response: {}", e);
            AppError::internal("Invalid response from Auth0")
        })?;

        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = Auth0ManagementService::new(
            "test.auth0.com".to_string(),
            "client_id".to_string(),
            "client_secret".to_string(),
        );
        assert_eq!(service.domain, "test.auth0.com");
        assert_eq!(service.client_id, "client_id");
    }

    #[test]
    fn test_user_metadata_serialization() {
        let metadata = UserMetadataUpdate {
            display_name: Some("Test User".to_string()),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("display_name"));
        assert!(json.contains("Test User"));
        assert!(json.contains("avatar_url"));
        assert!(json.contains("https://example.com/avatar.png"));

        // Test with None - should be omitted
        let metadata = UserMetadataUpdate {
            display_name: None,
            avatar_url: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(!json.contains("display_name"));
        assert!(!json.contains("avatar_url"));
    }
}
