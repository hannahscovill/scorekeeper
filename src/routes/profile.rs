//! User profile route handlers.
//! All user-editable profile data is stored in Auth0 user_metadata to protect
//! it from being overwritten by social login identity provider syncs.

use actix_multipart::Multipart;
use actix_web::{get, post, put, web, HttpResponse};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::middleware::auth::Claims;
use crate::models::error::AppError;
use crate::services::{Auth0ManagementService, S3AvatarService};

/// Request body for updating a profile.
/// All fields are optional - only provided fields will be updated.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pronouns: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Response from avatar upload endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarUploadResponse {
    pub avatar_url: String,
}

/// Resolve an avatar URL through the S3 service if available.
/// Returns the original value if S3 service is not configured or the value is not an S3 avatar.
async fn resolve_avatar(
    s3_service: &Option<web::Data<S3AvatarService>>,
    avatar_value: Option<String>,
) -> Option<String> {
    match (avatar_value, s3_service) {
        (Some(val), Some(s3)) => match s3.resolve_avatar_url(&val).await {
            Ok(resolved) => Some(resolved),
            Err(_) => Some(val),
        },
        (Some(val), None) => Some(val),
        (None, _) => None,
    }
}

/// GET /profile - Get the current user's profile from Auth0.
#[get("/profile")]
#[instrument(name = "get_profile", skip(auth0_service), fields(user_id = %claims.sub))]
pub async fn get_profile(
    claims: Claims,
    auth0_service: web::Data<Auth0ManagementService>,
    s3_service: Option<web::Data<S3AvatarService>>,
) -> Result<HttpResponse, AppError> {
    let user = auth0_service.get_user(&claims.sub).await?;

    let metadata = user.user_metadata.as_ref();
    let display_name = metadata.map(|m| m.display_name.clone());
    let name = metadata.and_then(|m| m.name.clone());
    let pronouns = metadata.and_then(|m| m.pronouns.clone());

    // Avatar resolution chain: user_metadata.avatar_url → Auth0 root picture → null
    let raw_avatar = metadata
        .and_then(|m| m.avatar_url.clone())
        .or_else(|| user.picture.clone());
    let avatar_url = resolve_avatar(&s3_service, raw_avatar).await;

    Ok(HttpResponse::Ok().json(ProfileResponse {
        user_id: user.user_id,
        display_name,
        avatar_url,
        name,
        pronouns,
        email: user.email,
    }))
}

/// PUT /profile - Update the current user's profile in Auth0.
#[put("/profile")]
#[instrument(name = "update_profile", skip(body, auth0_service), fields(user_id = %claims.sub))]
pub async fn update_profile(
    claims: Claims,
    body: web::Json<UpdateProfileRequest>,
    auth0_service: web::Data<Auth0ManagementService>,
    s3_service: Option<web::Data<S3AvatarService>>,
) -> Result<HttpResponse, AppError> {
    // Must have at least one field to update
    if body.display_name.is_none()
        && body.avatar_url.is_none()
        && body.name.is_none()
        && body.pronouns.is_none()
    {
        return Err(AppError::bad_request(
            "At least one field (displayName, avatarUrl, name, or pronouns) must be provided",
        ));
    }

    let user = auth0_service
        .update_user_metadata(
            &claims.sub,
            body.display_name.as_deref(),
            body.avatar_url.as_deref(),
            body.name.as_deref(),
            body.pronouns.as_deref(),
        )
        .await?;

    let metadata = user.user_metadata.as_ref();
    let display_name = metadata.map(|m| m.display_name.clone());
    let name = metadata.and_then(|m| m.name.clone());
    let pronouns = metadata.and_then(|m| m.pronouns.clone());
    let raw_avatar = metadata
        .and_then(|m| m.avatar_url.clone())
        .or_else(|| user.picture.clone());
    let avatar_url = resolve_avatar(&s3_service, raw_avatar).await;

    Ok(HttpResponse::Ok().json(ProfileResponse {
        user_id: user.user_id,
        display_name,
        avatar_url,
        name,
        pronouns,
        email: user.email,
    }))
}

/// POST /profile/avatar - Upload an avatar image.
#[post("/profile/avatar")]
#[instrument(name = "upload_avatar", skip(payload, s3_service, auth0_service), fields(user_id = %claims.sub))]
pub async fn upload_avatar(
    claims: Claims,
    mut payload: Multipart,
    s3_service: web::Data<S3AvatarService>,
    auth0_service: web::Data<Auth0ManagementService>,
) -> Result<HttpResponse, AppError> {
    // Find the "avatar" field in the multipart payload
    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(item) = payload.next().await {
        let mut field = item
            .map_err(|e| AppError::bad_request(format!("Failed to read multipart field: {}", e)))?;

        // Check if this is the "avatar" field
        let field_name = field.name().map(|s| s.to_string());
        if field_name.as_deref() != Some("avatar") {
            continue;
        }

        // Get content type
        let content_type = field
            .content_type()
            .map(|ct| ct.to_string())
            .unwrap_or_default();

        // Read all chunks into a buffer
        let mut data = Vec::new();
        while let Some(chunk) = field.next().await {
            let chunk = chunk
                .map_err(|e| AppError::bad_request(format!("Failed to read file chunk: {}", e)))?;
            data.extend_from_slice(&chunk);
        }

        file_data = Some((data, content_type));
        break;
    }

    // Ensure we got a file
    let (data, content_type) =
        file_data.ok_or_else(|| AppError::bad_request("Missing 'avatar' field in request"))?;

    // Upload to S3 (returns the S3 key, not a full URL)
    let avatar_key = s3_service
        .upload_avatar(&claims.sub, data, &content_type)
        .await?;

    // Update Auth0 user metadata with the S3 key
    auth0_service
        .update_user_metadata(&claims.sub, None, Some(&avatar_key), None, None)
        .await?;

    // Generate a pre-signed URL for the response so the frontend can display it immediately
    let avatar_url = s3_service.get_presigned_url(&avatar_key).await?;

    Ok(HttpResponse::Ok().json(AvatarUploadResponse { avatar_url }))
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
        assert_eq!(request.name, None);
        assert_eq!(request.pronouns, None);

        // Test with just avatarUrl
        let json = r#"{"avatarUrl": "https://example.com/avatar.png"}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, None);
        assert_eq!(
            request.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );

        // Test with all fields
        let json = r#"{"displayName": "hscov", "name": "Hannah", "pronouns": "she/her", "avatarUrl": "https://example.com/avatar.png"}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, Some("hscov".to_string()));
        assert_eq!(request.name, Some("Hannah".to_string()));
        assert_eq!(request.pronouns, Some("she/her".to_string()));
        assert_eq!(
            request.avatar_url,
            Some("https://example.com/avatar.png".to_string())
        );

        // Test with empty object
        let json = r#"{}"#;
        let request: UpdateProfileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.display_name, None);
        assert_eq!(request.avatar_url, None);
        assert_eq!(request.name, None);
        assert_eq!(request.pronouns, None);
    }

    #[test]
    fn test_profile_response_serialization() {
        let response = ProfileResponse {
            user_id: "auth0|123".to_string(),
            display_name: Some("Test User".to_string()),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            name: Some("Test Name".to_string()),
            pronouns: Some("they/them".to_string()),
            email: Some("test@example.com".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("userId"));
        assert!(json.contains("displayName"));
        assert!(json.contains("avatarUrl"));
        assert!(json.contains("\"name\""));
        assert!(json.contains("pronouns"));
        assert!(json.contains("email"));

        // Test with None fields - should be omitted
        let response = ProfileResponse {
            user_id: "auth0|123".to_string(),
            display_name: None,
            avatar_url: None,
            name: None,
            pronouns: None,
            email: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("userId"));
        assert!(!json.contains("displayName"));
        assert!(!json.contains("avatarUrl"));
        assert!(!json.contains("\"name\""));
        assert!(!json.contains("pronouns"));
        assert!(!json.contains("email"));
    }
}
