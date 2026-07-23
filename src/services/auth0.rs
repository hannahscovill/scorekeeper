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

/// Request body for creating a new passwordless "email" connection user.
#[derive(Debug, Serialize)]
struct CreateUserRequest {
    connection: String,
    email: String,
    email_verified: bool,
    user_metadata: UserMetadataUpdate,
}

/// User metadata for updates (all fields optional).
#[derive(Debug, Serialize)]
struct UserMetadataUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pronouns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opt_in_test_track_ios: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opt_in_test_track_ios_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opt_in_test_track_android: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opt_in_test_track_android_at: Option<String>,
}

/// User metadata from Auth0.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserMetadata {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pronouns: Option<String>,
    #[serde(default)]
    pub opt_in_test_track_ios: bool,
    #[serde(default)]
    pub opt_in_test_track_ios_at: Option<String>,
    #[serde(default)]
    pub opt_in_test_track_android: bool,
    #[serde(default)]
    pub opt_in_test_track_android_at: Option<String>,
}

/// A single Auth0 identity (one per linked connection, e.g. "email" passwordless,
/// a social provider, or a database connection).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Identity {
    pub connection: String,
}

/// Response from Auth0 user endpoints.
/// Includes both root-level fields and user_metadata.
#[derive(Debug, Deserialize)]
pub struct Auth0User {
    pub user_id: String,
    /// Auth0 root `name` field (often set by identity provider).
    #[serde(default)]
    pub name: Option<String>,
    /// Auth0 root `email` field.
    #[serde(default)]
    pub email: Option<String>,
    /// Auth0 root `picture` field (often a Gravatar/social provider URL).
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub user_metadata: Option<UserMetadata>,
    /// Linked identities — used to confirm a `users-by-email` match is the
    /// passwordless "email" connection lead, not an unrelated account that
    /// happens to share the same email address.
    #[serde(default)]
    pub identities: Vec<Identity>,
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
        name: Option<&str>,
        pronouns: Option<&str>,
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
                name: name.map(|s| s.to_string()),
                pronouns: pronouns.map(|s| s.to_string()),
                opt_in_test_track_ios: None,
                opt_in_test_track_ios_at: None,
                opt_in_test_track_android: None,
                opt_in_test_track_android_at: None,
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

    /// Sets the internal test track opt-in flags for a user. Only the
    /// platform(s) passed as `Some(timestamp)` are written — the other stays
    /// untouched (Auth0's shallow merge on `user_metadata` means an existing
    /// `true` from a prior call is never overwritten by an omitted field).
    ///
    /// Shared by both the authenticated profile flow (`user_id` from a JWT)
    /// and the anonymous passwordless-lead flow (`user_id` from an
    /// email-connection lookup/creation) — this is the one place that
    /// actually writes opt-in state, regardless of how the user_id was
    /// obtained.
    pub async fn set_test_track_opt_in(
        &self,
        user_id: &str,
        ios: Option<&str>,
        android: Option<&str>,
    ) -> Result<Auth0User, AppError> {
        let token = self.get_access_token().await?;

        let url = format!(
            "https://{}/api/v2/users/{}",
            self.domain,
            urlencoding::encode(user_id)
        );

        let request_body = UpdateUserRequest {
            user_metadata: UserMetadataUpdate {
                display_name: None,
                avatar_url: None,
                name: None,
                pronouns: None,
                opt_in_test_track_ios: ios.map(|_| true),
                opt_in_test_track_ios_at: ios.map(|s| s.to_string()),
                opt_in_test_track_android: android.map(|_| true),
                opt_in_test_track_android_at: android.map(|s| s.to_string()),
            },
        };

        debug!("Setting test track opt-in for user: {}", user_id);

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
            error!(
                "Auth0 test track opt-in update failed: {} - {}",
                status, body
            );

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

    /// Looks up an existing passwordless "email" connection user by email.
    /// Returns `None` if no such user exists yet (a brand-new signup).
    /// Filters out any match that isn't actually on the "email" connection,
    /// so this never matches an unrelated real login account that happens
    /// to share the same email address.
    pub async fn find_email_connection_user(
        &self,
        email: &str,
    ) -> Result<Option<Auth0User>, AppError> {
        let token = self.get_access_token().await?;

        let url = format!(
            "https://{}/api/v2/users-by-email?email={}",
            self.domain,
            urlencoding::encode(email)
        );

        debug!("Looking up email-connection user in Auth0");

        let response = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to look up user by email in Auth0: {}", e);
                AppError::internal("Failed to communicate with Auth0")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Auth0 users-by-email lookup failed: {} - {}", status, body);

            return match status.as_u16() {
                401 | 403 => Err(AppError::internal(
                    "Auth0 Management API authorization failed",
                )),
                _ => Err(AppError::internal("Failed to look up user in Auth0")),
            };
        }

        let users: Vec<Auth0User> = response.json().await.map_err(|e| {
            error!("Failed to parse users-by-email response: {}", e);
            AppError::internal("Invalid response from Auth0")
        })?;

        Ok(users
            .into_iter()
            .find(|u| u.identities.iter().any(|i| i.connection == "email")))
    }

    /// Creates a new passwordless "email" connection user — no password, never
    /// used to log in — with the given opt-in fields set on `user_metadata`.
    pub async fn create_email_connection_lead(
        &self,
        email: &str,
        ios: Option<&str>,
        android: Option<&str>,
    ) -> Result<Auth0User, AppError> {
        let token = self.get_access_token().await?;

        let url = format!("https://{}/api/v2/users", self.domain);

        let request_body = CreateUserRequest {
            connection: "email".to_string(),
            email: email.to_string(),
            email_verified: false,
            user_metadata: UserMetadataUpdate {
                display_name: None,
                avatar_url: None,
                name: None,
                pronouns: None,
                opt_in_test_track_ios: ios.map(|_| true),
                opt_in_test_track_ios_at: ios.map(|s| s.to_string()),
                opt_in_test_track_android: android.map(|_| true),
                opt_in_test_track_android_at: android.map(|s| s.to_string()),
            },
        };

        debug!("Creating email-connection lead in Auth0");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to create user in Auth0: {}", e);
                AppError::internal("Failed to communicate with Auth0")
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Auth0 user creation failed: {} - {}", status, body);

            return match status.as_u16() {
                401 | 403 => Err(AppError::internal(
                    "Auth0 Management API authorization failed",
                )),
                409 => Err(AppError::internal("User already exists in Auth0")),
                _ => Err(AppError::internal("Failed to create user in Auth0")),
            };
        }

        let user: Auth0User = response.json().await.map_err(|e| {
            error!("Failed to parse user creation response: {}", e);
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
            name: Some("Test Name".to_string()),
            pronouns: Some("they/them".to_string()),
            opt_in_test_track_ios: None,
            opt_in_test_track_ios_at: None,
            opt_in_test_track_android: None,
            opt_in_test_track_android_at: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("display_name"));
        assert!(json.contains("Test User"));
        assert!(json.contains("avatar_url"));
        assert!(json.contains("https://example.com/avatar.png"));
        assert!(json.contains("\"name\""));
        assert!(json.contains("Test Name"));
        assert!(json.contains("pronouns"));
        assert!(json.contains("they/them"));
        assert!(!json.contains("opt_in_test_track"));

        // Test with None - should be omitted
        let metadata = UserMetadataUpdate {
            display_name: None,
            avatar_url: None,
            name: None,
            pronouns: None,
            opt_in_test_track_ios: None,
            opt_in_test_track_ios_at: None,
            opt_in_test_track_android: None,
            opt_in_test_track_android_at: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(!json.contains("display_name"));
        assert!(!json.contains("avatar_url"));
        assert!(!json.contains("\"name\""));
        assert!(!json.contains("pronouns"));
    }

    #[test]
    fn test_user_metadata_update_serialization_with_beta_fields() {
        let metadata = UserMetadataUpdate {
            display_name: None,
            avatar_url: None,
            name: None,
            pronouns: None,
            opt_in_test_track_ios: Some(true),
            opt_in_test_track_ios_at: Some("2026-07-22T10:00:00+00:00".to_string()),
            opt_in_test_track_android: None,
            opt_in_test_track_android_at: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("opt_in_test_track_ios"));
        assert!(json.contains("2026-07-22T10:00:00+00:00"));
        assert!(!json.contains("opt_in_test_track_android"));
        assert!(!json.contains("display_name"));
        assert!(!json.contains("avatar_url"));
        assert!(!json.contains("\"name\""));
        assert!(!json.contains("pronouns"));
    }

    #[test]
    fn test_user_metadata_deserialization_defaults_beta_fields_when_absent() {
        let json = r#"{"display_name": "hscov"}"#;
        let metadata: UserMetadata = serde_json::from_str(json).unwrap();
        assert!(!metadata.opt_in_test_track_ios);
        assert_eq!(metadata.opt_in_test_track_ios_at, None);
        assert!(!metadata.opt_in_test_track_android);
        assert_eq!(metadata.opt_in_test_track_android_at, None);
    }

    #[test]
    fn test_auth0_user_deserialization_with_root_fields() {
        let json = r#"{
            "user_id": "auth0|123",
            "name": "Hannah Scovill",
            "email": "hannah@example.com",
            "picture": "https://s.gravatar.com/avatar/abc123",
            "user_metadata": {
                "display_name": "hscov",
                "avatar_url": "avatars/auth0-123/1234567890.jpg",
                "name": "Hannah",
                "pronouns": "she/her"
            }
        }"#;
        let user: Auth0User = serde_json::from_str(json).unwrap();
        assert_eq!(user.user_id, "auth0|123");
        assert_eq!(user.name, Some("Hannah Scovill".to_string()));
        assert_eq!(user.email, Some("hannah@example.com".to_string()));
        assert_eq!(
            user.picture,
            Some("https://s.gravatar.com/avatar/abc123".to_string())
        );
        let meta = user.user_metadata.unwrap();
        assert_eq!(meta.display_name, "hscov");
        assert_eq!(meta.name, Some("Hannah".to_string()));
        assert_eq!(meta.pronouns, Some("she/her".to_string()));
    }

    #[test]
    fn test_auth0_user_deserialization_minimal() {
        let json = r#"{"user_id": "auth0|456"}"#;
        let user: Auth0User = serde_json::from_str(json).unwrap();
        assert_eq!(user.user_id, "auth0|456");
        assert_eq!(user.name, None);
        assert_eq!(user.email, None);
        assert_eq!(user.picture, None);
        assert!(user.user_metadata.is_none());
        assert!(user.identities.is_empty());
    }

    #[test]
    fn test_auth0_user_deserialization_with_beta_opt_in() {
        let json = r#"{
            "user_id": "auth0|123",
            "user_metadata": {
                "opt_in_test_track_ios": true,
                "opt_in_test_track_ios_at": "2026-07-22T10:00:00+00:00",
                "opt_in_test_track_android": true,
                "opt_in_test_track_android_at": "2026-07-23T09:00:00+00:00"
            }
        }"#;
        let user: Auth0User = serde_json::from_str(json).unwrap();
        let meta = user.user_metadata.unwrap();
        assert!(meta.opt_in_test_track_ios);
        assert_eq!(
            meta.opt_in_test_track_ios_at,
            Some("2026-07-22T10:00:00+00:00".to_string())
        );
        assert!(meta.opt_in_test_track_android);
        assert_eq!(
            meta.opt_in_test_track_android_at,
            Some("2026-07-23T09:00:00+00:00".to_string())
        );
    }

    #[test]
    fn test_auth0_user_deserialization_with_identities() {
        let json = r#"{
            "user_id": "email|abc123",
            "email": "lead@example.com",
            "identities": [{"connection": "email"}]
        }"#;
        let user: Auth0User = serde_json::from_str(json).unwrap();
        assert_eq!(user.identities.len(), 1);
        assert_eq!(user.identities[0].connection, "email");
    }

    #[test]
    fn test_find_email_connection_user_filters_by_connection() {
        // Simulates the filtering logic in find_email_connection_user: only a
        // user with an "email"-connection identity should be treated as a
        // match, never a real login account that happens to share the email.
        let json = r#"[
            {"user_id": "google-oauth2|999", "identities": [{"connection": "google-oauth2"}]},
            {"user_id": "email|abc123", "identities": [{"connection": "email"}]}
        ]"#;
        let users: Vec<Auth0User> = serde_json::from_str(json).unwrap();
        let matched = users
            .into_iter()
            .find(|u| u.identities.iter().any(|i| i.connection == "email"));
        assert_eq!(matched.unwrap().user_id, "email|abc123");
    }

    #[test]
    fn test_find_email_connection_user_no_match_when_only_other_connections() {
        let json = r#"[
            {"user_id": "google-oauth2|999", "identities": [{"connection": "google-oauth2"}]}
        ]"#;
        let users: Vec<Auth0User> = serde_json::from_str(json).unwrap();
        let matched = users
            .into_iter()
            .find(|u| u.identities.iter().any(|i| i.connection == "email"));
        assert!(matched.is_none());
    }

    #[test]
    fn test_create_user_request_serialization() {
        let request = CreateUserRequest {
            connection: "email".to_string(),
            email: "lead@example.com".to_string(),
            email_verified: false,
            user_metadata: UserMetadataUpdate {
                display_name: None,
                avatar_url: None,
                name: None,
                pronouns: None,
                opt_in_test_track_ios: Some(true),
                opt_in_test_track_ios_at: Some("2026-07-22T10:00:00+00:00".to_string()),
                opt_in_test_track_android: None,
                opt_in_test_track_android_at: None,
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"connection\":\"email\""));
        assert!(json.contains("\"email\":\"lead@example.com\""));
        assert!(json.contains("\"email_verified\":false"));
        assert!(json.contains("opt_in_test_track_ios"));
        assert!(!json.contains("opt_in_test_track_android"));
    }
}
