//! Service for loading and managing common words for puzzle selection.
//!
//! This service loads a curated list of common 5-letter words from either
//! a local file or S3. These words are used for random puzzle selection
//! while the full dictionary (words.txt) is still used for validating guesses.

use aws_sdk_s3::Client as S3Client;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::dictionary::is_valid_word;

/// Source for loading common words.
pub enum CommonWordsSource {
    /// Load from local file path.
    File(String),
    /// Load from S3 bucket and key.
    S3 {
        client: S3Client,
        bucket: String,
        key: String,
    },
}

/// Service for loading common words.
pub struct CommonWordsService {
    source: CommonWordsSource,
    /// Cached list of common words (validated against dictionary).
    words: Arc<RwLock<Option<Vec<String>>>>,
}

impl CommonWordsService {
    /// Creates a new CommonWordsService with the given source.
    pub fn new(source: CommonWordsSource) -> Self {
        Self {
            source,
            words: Arc::new(RwLock::new(None)),
        }
    }

    /// Loads common words and caches them.
    pub async fn load(&self) -> Result<(), String> {
        let content = match &self.source {
            CommonWordsSource::File(path) => {
                info!("Loading common words from file: {}", path);
                std::fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read common words file: {}", e))?
            }
            CommonWordsSource::S3 {
                client,
                bucket,
                key,
            } => {
                info!("Loading common words from S3: s3://{}/{}", bucket, key);
                let response = client
                    .get_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|e| format!("Failed to fetch from S3: {}", e))?;

                let body = response
                    .body
                    .collect()
                    .await
                    .map_err(|e| format!("Failed to read S3 response: {}", e))?;

                String::from_utf8(body.into_bytes().to_vec())
                    .map_err(|e| format!("Invalid UTF-8: {}", e))?
            }
        };

        let unique_words = Self::parse_words(&content);

        if unique_words.is_empty() {
            warn!("No valid common words loaded!");
        }

        let mut cache = self.words.write().await;
        *cache = Some(unique_words);

        Ok(())
    }

    /// Parses words from content string, validating against dictionary.
    fn parse_words(content: &str) -> Vec<String> {
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
        let mut seen = HashSet::new();
        let mut unique_words = Vec::new();
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

        unique_words
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

    #[test]
    fn test_parse_words() {
        let content = "crane\napple\nzzzzz\nab\n# comment\n\nvalid";
        let words = CommonWordsService::parse_words(content);
        assert!(words.contains(&"crane".to_string()));
        assert!(words.contains(&"apple".to_string()));
        assert!(words.contains(&"valid".to_string()));
        assert!(!words.contains(&"zzzzz".to_string())); // not in dictionary
        assert!(!words.contains(&"ab".to_string())); // wrong length
    }

    #[test]
    fn test_word_validation_logic() {
        assert!(is_valid_word("crane"));
        assert!(is_valid_word("apple"));
        assert!(!is_valid_word("zzzzz"));
    }
}
