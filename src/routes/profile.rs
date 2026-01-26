//! User profile route handlers.
//! All profile data is stored in Auth0 user_metadata.

use actix_web::{get, put, web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::Claims;
use crate::models::error::AppError;
use crate::services::Auth0ManagementService;

/// Request body for updating a profile.
/// All fields are optional - only provided fields will be updated.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// Response from profile endpoints.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// GET /profile - Get the current user's profile from Auth0.
#[get("/profile")]
pub async fn get_profile(
    claims: Claims,
    auth0_service: web::Data<Auth0ManagementService>,
) -> Result<HttpResponse, AppError> {
    let user = auth0_service.get_user(&claims.sub).await?;

    Ok(HttpResponse::Ok().json(ProfileResponse {
        user_id: user.user_id,
        display_name: user.user_metadata.as_ref().map(|m| m.display_name.clone()),
        avatar_url: user.user_metadata.and_then(|m| m.avatar_url),
    }))
}

/// PUT /profile - Update the current user's profile in Auth0.
#[put("/profile")]
pub async fn update_profile(
    claims: Claims,
    body: web::Json<UpdateProfileRequest>,
    auth0_service: web::Data<Auth0ManagementService>,
) -> Result<HttpResponse, AppError> {
    // Must have at least one field to update
    if body.display_name.is_none() && body.avatar_url.is_none() {
        return Err(AppError::bad_request(
            "At least one field (displayName or avatarUrl) must be provided",
        ));
    }

    let user = auth0_service
        .update_user_metadata(
            &claims.sub,
            body.display_name.as_deref(),
            body.avatar_url.as_deref(),
        )
        .await?;

    Ok(HttpResponse::Ok().json(ProfileResponse {
        user_id: user.user_id,
        display_name: user.user_metadata.as_ref().map(|m| m.display_name.clone()),
        avatar_url: user.user_metadata.and_then(|m| m.avatar_url),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_profile_request_deserialization() {
        // Test with just displayName
        let json = r#"{"displayName": "Test User"}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, Some("Test User".to_string()));
        assert_eq!(request.avatar_url, None);

        // Test with just avatarUrl
        let json = r#"{"avatarUrl": "https://example.com/avatar.png"}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, None);
        assert_eq!(
            request.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );

        // Test with both fields
        let json = r#"{"displayName": "Test User", "avatarUrl": "https://example.com/avatar.png"}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, Some("Test User".to_string()));
        assert_eq!(
            request.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );

        // Test with empty object
        let json = r#"{}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, None);
        assert_eq!(request.avatar_url, None);
    }

    #[test]
    fn test_profile_response_serialization() {
        let response = ProfileResponse {
            user_id: "auth0|123".to_string(),
            display_name: Some("Test User".to_string()),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("userId"));
        assert!(json.contains("displayName"));
        assert!(json.contains("avatarUrl"));

        // Test with None fields - should be omitted
        let response = ProfileResponse {
            user_id: "auth0|123".to_string(),
            display_name: None,
            avatar_url: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("userId"));
        assert!(!json.contains("displayName"));
        assert!(!json.contains("avatarUrl"));
    }
}
