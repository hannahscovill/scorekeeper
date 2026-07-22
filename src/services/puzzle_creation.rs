//! Service for creating puzzles from randomly selected, previously unused words.

use chrono::NaiveDate;
use rand::seq::IteratorRandom;
use std::collections::HashSet;
use std::sync::Arc;

use crate::db::PuzzleDatabase;
use crate::models::error::AppError;
use crate::services::CommonWordsService;

/// Picks a random word from the common words list that hasn't been used in
/// any existing puzzle yet, and sets it as the puzzle answer for `date`.
///
/// Returns the selected word. Shared by the admin "set puzzle" endpoint
/// (`set_random_unused_word`) and by gameplay endpoints that fall back to
/// creating a puzzle on demand when a player requests a date with none set.
pub async fn create_random_puzzle(
    puzzle_db: &Arc<dyn PuzzleDatabase>,
    common_words: &CommonWordsService,
    date: NaiveDate,
    team_id: Option<&str>,
) -> Result<String, AppError> {
    let existing_puzzles = puzzle_db
        .get_puzzle_answers(None, None)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    let used_words: HashSet<String> = existing_puzzles
        .into_iter()
        .map(|p| p.word.to_lowercase())
        .collect();

    let words = common_words
        .get_words()
        .await
        .ok_or_else(|| AppError::InternalError("Common words list failed to load".to_string()))?;

    let mut rng = rand::thread_rng();
    let unused: Vec<_> = words.iter().filter(|w| !used_words.contains(*w)).collect();

    let word = unused
        .into_iter()
        .choose(&mut rng)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AppError::bad_request("No unused words available in the common words list")
        })?;

    puzzle_db
        .set_puzzle_answer(date, &word, team_id)
        .await
        .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    Ok(word)
}
