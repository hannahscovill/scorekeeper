//! S3 Avatar upload service.

use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::presigning::PresigningConfig;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

use crate::models::error::AppError;

/// Maximum file size for avatar uploads (2MB).
const MAX_FILE_SIZE: usize = 2 * 1024 * 1024;

/// Allowed content types for avatar uploads.
const ALLOWED_CONTENT_TYPES: &[&str] = &["image/jpeg", "image/png"];

/// Pre-signed URL expiry duration (1 hour).
const PRESIGNED_URL_EXPIRY: Duration = Duration::from_secs(3600);

/// S3 URL prefix for the scorekeeper-avatars bucket.
const S3_BUCKET_URL_PREFIX: &str = "https://scorekeeper-avatars.s3.amazonaws.com/";

/// Service for uploading avatars to S3.
pub struct S3AvatarService {
    client: S3Client,
    bucket: String,
}

impl S3AvatarService {
    /// Creates a new S3AvatarService.
    pub fn new(client: S3Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Validates the content type is JPEG or PNG.
    pub fn validate_content_type(content_type: &str) -> Result<(), AppError> {
        if ALLOWED_CONTENT_TYPES.contains(&content_type) {
            Ok(())
        } else {
            Err(AppError::bad_request(
                "Invalid file type. Only JPEG and PNG images are allowed.",
            ))
        }
    }

    /// Validates the file size is within limits.
    pub fn validate_file_size(size: usize) -> Result<(), AppError> {
        if size == 0 {
            Err(AppError::bad_request("Empty file"))
        } else if size > MAX_FILE_SIZE {
            Err(AppError::bad_request(
                "File too large. Maximum size is 2MB.",
            ))
        } else {
            Ok(())
        }
    }

    /// Uploads an avatar to S3 and returns the S3 key.
    pub async fn upload_avatar(
        &self,
        user_id: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, AppError> {
        // Validate inputs
        Self::validate_content_type(content_type)?;
        Self::validate_file_size(data.len())?;

        // Sanitize user_id for use in S3 key (replace | with -)
        let sanitized_user_id = user_id.replace('|', "-");

        // Generate timestamp for cache busting
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        // Determine file extension from content type
        let ext = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            _ => return Err(AppError::bad_request("Invalid content type")),
        };

        // Build S3 key
        let key = format!("avatars/{}/{}.{}", sanitized_user_id, timestamp, ext);

        debug!(
            "Uploading avatar to S3: bucket={}, key={}",
            self.bucket, key
        );

        // Upload to S3
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(data.into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to upload avatar to S3: {}", e);
                AppError::internal("Failed to upload avatar")
            })?;

        debug!("Avatar uploaded successfully: key={}", key);
        Ok(key)
    }

    /// Generates a pre-signed GET URL for the given S3 key.
    pub async fn get_presigned_url(&self, key: &str) -> Result<String, AppError> {
        let presigning_config = PresigningConfig::expires_in(PRESIGNED_URL_EXPIRY).map_err(|e| {
            error!("Failed to create presigning config: {}", e);
            AppError::internal("Failed to generate avatar URL")
        })?;

        let presigned_request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|e| {
                error!("Failed to generate pre-signed URL: {}", e);
                AppError::internal("Failed to generate avatar URL")
            })?;

        Ok(presigned_request.uri().to_string())
    }

    /// Extracts the S3 key from an avatar value.
    ///
    /// Handles both legacy full URLs (`https://scorekeeper-avatars.s3.amazonaws.com/avatars/...`)
    /// and new bare keys (`avatars/...`). Returns `None` for non-S3 URLs (Gravatar, Auth0 picture).
    pub fn extract_s3_key(avatar_value: &str) -> Option<String> {
        if avatar_value.is_empty() {
            return None;
        }

        // Legacy full S3 URL
        if let Some(key) = avatar_value.strip_prefix(S3_BUCKET_URL_PREFIX) {
            if !key.is_empty() {
                return Some(key.to_string());
            }
            return None;
        }

        // Bare S3 key (starts with "avatars/")
        if avatar_value.starts_with("avatars/") {
            return Some(avatar_value.to_string());
        }

        // Non-S3 URL (Gravatar, Auth0 picture, etc.)
        None
    }

    /// Resolves an avatar value to a displayable URL.
    ///
    /// If the value is an S3 key or legacy S3 URL, generates a pre-signed URL.
    /// Otherwise returns the original value unchanged (e.g. Gravatar URLs).
    pub async fn resolve_avatar_url(&self, avatar_value: &str) -> Result<String, AppError> {
        match Self::extract_s3_key(avatar_value) {
            Some(key) => self.get_presigned_url(&key).await,
            None => Ok(avatar_value.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_content_type_jpeg() {
        assert!(S3AvatarService::validate_content_type("image/jpeg").is_ok());
    }

    #[test]
    fn test_validate_content_type_png() {
        assert!(S3AvatarService::validate_content_type("image/png").is_ok());
    }

    #[test]
    fn test_validate_content_type_invalid() {
        let result = S3AvatarService::validate_content_type("image/gif");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_size_valid() {
        assert!(S3AvatarService::validate_file_size(1024).is_ok());
        assert!(S3AvatarService::validate_file_size(MAX_FILE_SIZE).is_ok());
    }

    #[test]
    fn test_validate_file_size_empty() {
        let result = S3AvatarService::validate_file_size(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_size_too_large() {
        let result = S3AvatarService::validate_file_size(MAX_FILE_SIZE + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_s3_key_legacy_url() {
        let url = "https://scorekeeper-avatars.s3.amazonaws.com/avatars/auth0-123/1234567890.jpg";
        let key = S3AvatarService::extract_s3_key(url);
        assert_eq!(key, Some("avatars/auth0-123/1234567890.jpg".to_string()));
    }

    #[test]
    fn test_extract_s3_key_bare_key() {
        let key_input = "avatars/auth0-123/1234567890.png";
        let key = S3AvatarService::extract_s3_key(key_input);
        assert_eq!(key, Some("avatars/auth0-123/1234567890.png".to_string()));
    }

    #[test]
    fn test_extract_s3_key_gravatar() {
        let url = "https://www.gravatar.com/avatar/abc123?d=mp";
        let key = S3AvatarService::extract_s3_key(url);
        assert_eq!(key, None);
    }

    #[test]
    fn test_extract_s3_key_auth0_picture() {
        let url = "https://s.gravatar.com/avatar/abc123?s=480&r=pg&d=https%3A%2F%2Fcdn.auth0.com";
        let key = S3AvatarService::extract_s3_key(url);
        assert_eq!(key, None);
    }

    #[test]
    fn test_extract_s3_key_empty_string() {
        let key = S3AvatarService::extract_s3_key("");
        assert_eq!(key, None);
    }
}
