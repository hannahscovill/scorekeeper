//! S3 Avatar upload service.

use aws_sdk_s3::Client as S3Client;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error};

use crate::models::error::AppError;

/// Maximum file size for avatar uploads (2MB).
const MAX_FILE_SIZE: usize = 2 * 1024 * 1024;

/// Allowed content types for avatar uploads.
const ALLOWED_CONTENT_TYPES: &[&str] = &["image/jpeg", "image/png"];

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

    /// Uploads an avatar to S3 and returns the URL.
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

        // Build and return the URL
        let url = format!("https://{}.s3.amazonaws.com/{}", self.bucket, key);

        debug!("Avatar uploaded successfully: {}", url);
        Ok(url)
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
}
