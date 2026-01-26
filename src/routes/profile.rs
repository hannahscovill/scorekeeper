//! User profile route handlers.

use actix_web::{get, put, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

use crate::middleware::auth::Claims;
use crate::models::error::AppError;

/// User profile data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
}

/// Request body for updating a profile.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequest {
    pub display_name: String,
    pub avatar_url: String,
}

/// In-memory profile store (for development).
/// In production, this would be replaced with DynamoDB or another persistent store.
pub struct ProfileStore {
    profiles: RwLock<HashMap<String, UserProfile>>,
}

impl ProfileStore {
    pub fn new() -> Self {
        Self {
            profiles: RwLock::new(HashMap::new()),
        }
    }

    pub fn get(&self, user_id: &str) -> Option<UserProfile> {
        self.profiles.read().ok()?.get(user_id).cloned()
    }

    pub fn set(&self, profile: UserProfile) -> Result<(), String> {
        let mut profiles = self.profiles.write().map_err(|e| e.to_string())?;
        profiles.insert(profile.user_id.clone(), profile);
        Ok(())
    }
}

impl Default for ProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

/// GET /profile - Get the current user's profile.
#[get("/profile")]
pub async fn get_profile(
    claims: Claims,
    store: web::Data<ProfileStore>,
) -> Result<HttpResponse, AppError> {
    match store.get(&claims.sub) {
        Some(profile) => Ok(HttpResponse::Ok().json(profile)),
        None => Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Profile not found"
        }))),
    }
}

/// PUT /profile - Update the current user's profile.
#[put("/profile")]
pub async fn update_profile(
    claims: Claims,
    body: web::Json<UpdateProfileRequest>,
    store: web::Data<ProfileStore>,
) -> Result<HttpResponse, AppError> {
    let profile = UserProfile {
        user_id: claims.sub.clone(),
        display_name: body.display_name.clone(),
        avatar_url: body.avatar_url.clone(),
    };

    store
        .set(profile.clone())
        .map_err(AppError::InternalError)?;

    Ok(HttpResponse::Ok().json(profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_store() {
        let store = ProfileStore::new();

        // Initially empty
        assert!(store.get("user1").is_none());

        // Add a profile
        let profile = UserProfile {
            user_id: "user1".to_string(),
            display_name: "Test User".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
        };
        store.set(profile.clone()).unwrap();

        // Retrieve it
        let retrieved = store.get("user1").unwrap();
        assert_eq!(retrieved.display_name, "Test User");
    }
}
