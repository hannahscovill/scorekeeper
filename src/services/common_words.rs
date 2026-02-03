//! Service for loading and managing common words for puzzle selection.
//!
//! This service fetches a curated list of common 5-letter words from S3.
//! These words are used for random puzzle selection while the full dictionary
//! (words.txt) is still used for validating user guesses.

use aws_sdk_s3::Client as S3Client;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::dictionary::is_valid_word;

/// Service for loading common words from S3.
pub struct CommonWordsService {
    client: S3Client,
    bucket: String,
    key: String,
    /// Cached list of common words (validated against dictionary).
    words: Arc<RwLock<Option<Vec<String>>>>,
}

impl CommonWordsService {
    /// Creates a new CommonWordsService.
    pub fn new(client: S3Client, bucket: String, key: String) -> Self {
        Self {
            client,
            bucket,
            key,
            words: Arc::new(RwLock::new(None)),
        }
    }

    /// Loads common words from S3 and caches them.
    /// Only words that exist in the main dictionary are included.
    pub async fn load(&self) -> Result<(), String> {
        info!(
            "Loading common words from S3: s3://{}/{}",
            self.bucket, self.key
        );

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&self.key)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to fetch common words from S3: {}", e);
                format!("Failed to fetch common words: {}", e)
            })?;

        let body = response.body.collect().await.map_err(|e| {
            error!("Failed to read S3 response body: {}", e);
            format!("Failed to read response: {}", e)
        })?;

        let content = String::from_utf8(body.into_bytes().to_vec()).map_err(|e| {
            error!("Invalid UTF-8 in common words file: {}", e);
            format!("Invalid UTF-8: {}", e)
        })?;

        // Parse words: one per line, skip comments and empty lines
        let mut valid_words = Vec::new();
        let mut skipped_invalid = 0;
        let mut skipped_not_in_dict = 0;

        for line in content.lines() {
            let word = line.trim().to_lowercase();

            // Skip empty lines and comments
            if word.is_empty() || word.starts_with('#') {
                continue;
            }

            // Validate word format: exactly 5 lowercase letters
            if word.len() != 5 || !word.chars().all(|c| c.is_ascii_lowercase()) {
                skipped_invalid += 1;
                debug!("Skipping invalid word format: {}", word);
                continue;
            }

            // Validate word exists in main dictionary
            if !is_valid_word(&word) {
                skipped_not_in_dict += 1;
                debug!("Skipping word not in dictionary: {}", word);
                continue;
            }

            valid_words.push(word);
        }

        // Remove duplicates while preserving order
        let seen: HashSet<_> = HashSet::new();
        let mut unique_words = Vec::new();
        let mut seen = seen;
        for word in valid_words {
            if seen.insert(word.clone()) {
                unique_words.push(word);
            }
        }

        info!(
            "Loaded {} common words ({} invalid format, {} not in dictionary)",
            unique_words.len(),
            skipped_invalid,
            skipped_not_in_dict
        );

        if unique_words.is_empty() {
            warn!("No valid common words loaded from S3!");
        }

        // Cache the words
        let mut cache = self.words.write().await;
        *cache = Some(unique_words);

        Ok(())
    }

    /// Returns the cached common words, or None if not loaded.
    pub async fn get_words(&self) -> Option<Vec<String>> {
        let cache = self.words.read().await;
        cache.clone()
    }

    /// Returns the number of cached common words.
    pub async fn word_count(&self) -> usize {
        let cache = self.words.read().await;
        cache.as_ref().map(|w| w.len()).unwrap_or(0)
    }

    /// Checks if the service has loaded words.
    pub async fn is_loaded(&self) -> bool {
        let cache = self.words.read().await;
        cache.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Integration tests with S3 would require mocking the S3 client
    // These unit tests focus on the word validation logic

    #[test]
    fn test_word_validation_logic() {
        // Test cases for word format validation
        assert!(is_valid_word("crane")); // Valid common word
        assert!(is_valid_word("apple")); // Valid common word
        assert!(!is_valid_word("zzzzz")); // Invalid - not in dictionary
    }
}
