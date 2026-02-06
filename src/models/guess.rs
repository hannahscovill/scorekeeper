//! Data structures for word guessing game.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request payload for submitting a guess.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuessRequest {
    /// The date of the puzzle in ISO format (YYYY-MM-DD).
    pub puzzle_date_iso_day: NaiveDate,
    /// The 5-letter word guessed by the player.
    pub word_guessed: String,
}

impl GuessRequest {
    /// Creates a new guess request.
    pub fn new(puzzle_date: NaiveDate, word: impl Into<String>) -> Self {
        Self {
            puzzle_date_iso_day: puzzle_date,
            word_guessed: word.into(),
        }
    }
}

/// Grade for a single letter in a guess.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LetterGrade {
    /// Letter is in the correct position.
    Correct,
    /// Letter is in the word but wrong position.
    Contained,
    /// Letter is not in the word.
    Wrong,
}

/// A graded letter with its grade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GradedLetter {
    /// The letter character.
    pub letter: char,
    /// The grade for this letter.
    pub grade: LetterGrade,
}

impl GradedLetter {
    /// Creates a new graded letter.
    pub fn new(letter: char, grade: LetterGrade) -> Self {
        Self { letter, grade }
    }
}

/// A graded guess consisting of 5 graded letters.
pub type GradedGuess = Vec<GradedLetter>;

/// Metadata about a game.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameMetadata {
    /// Unique identifier for this game.
    pub game_id: Uuid,
    /// User who played the game.
    pub user_id: String,
    /// Number of guesses made (1-6).
    pub moves_qty: u8,
    /// Whether the player won.
    pub won: bool,
}

/// A fully graded game response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GradedGame {
    /// Game metadata.
    #[serde(flatten)]
    pub metadata: GameMetadata,
    /// List of graded guesses.
    pub moves: Vec<GradedGuess>,
    /// The puzzle answer, only revealed when the game ends in a loss.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

impl GradedGame {
    /// Creates a new graded game.
    pub fn new(
        game_id: Uuid,
        user_id: impl Into<String>,
        moves: Vec<GradedGuess>,
        won: bool,
        answer: Option<String>,
    ) -> Self {
        Self {
            metadata: GameMetadata {
                game_id,
                user_id: user_id.into(),
                moves_qty: moves.len() as u8,
                won,
            },
            moves,
            answer,
        }
    }
}

/// Persisted game state for a user's progress on a puzzle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameState {
    /// User who is playing (Auth0 subject).
    pub user_id: String,
    /// The puzzle date (YYYY-MM-DD).
    pub puzzle_date: NaiveDate,
    /// The guesses made so far (raw words).
    pub guesses: Vec<String>,
    /// Whether the player has won.
    pub won: bool,
    /// When the game was started.
    pub created_at: DateTime<Utc>,
    /// When the game was last updated.
    pub updated_at: DateTime<Utc>,
}

impl GameState {
    /// Creates a new game state for a user starting a puzzle.
    pub fn new(user_id: impl Into<String>, puzzle_date: NaiveDate) -> Self {
        let now = Utc::now();
        Self {
            user_id: user_id.into(),
            puzzle_date,
            guesses: Vec::new(),
            won: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Adds a guess to the game state.
    pub fn add_guess(&mut self, guess: impl Into<String>) {
        self.guesses.push(guess.into());
        self.updated_at = Utc::now();
    }

    /// Marks the game as won.
    pub fn mark_won(&mut self) {
        self.won = true;
        self.updated_at = Utc::now();
    }

    /// Returns true if the game is still in progress (not won and < 6 guesses).
    pub fn is_in_progress(&self) -> bool {
        !self.won && self.guesses.len() < 6
    }

    /// Returns the number of guesses made.
    pub fn guess_count(&self) -> usize {
        self.guesses.len()
    }

    /// Returns the DynamoDB partition key for this game state.
    pub fn pk(&self) -> String {
        format!("USER#{}#PUZZLE#{}", self.user_id, self.puzzle_date)
    }

    /// Returns the DynamoDB sort key for this game state.
    pub fn sk() -> &'static str {
        "GAME"
    }
}

/// Puzzle answer for a specific date.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PuzzleAnswer {
    /// The puzzle date.
    pub puzzle_date: NaiveDate,
    /// The answer word.
    pub word: String,
}

impl PuzzleAnswer {
    /// Returns the DynamoDB partition key for this puzzle.
    pub fn pk(date: NaiveDate) -> String {
        format!("PUZZLE#{}", date)
    }

    /// Returns the DynamoDB sort key for puzzle answers.
    pub fn sk() -> &'static str {
        "ANSWER"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guess_request() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let req = GuessRequest::new(date, "crane");
        assert_eq!(req.puzzle_date_iso_day, date);
        assert_eq!(req.word_guessed, "crane");
    }

    #[test]
    fn test_guess_request_serialization() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let req = GuessRequest::new(date, "crane");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("2026-01-15"));
        assert!(json.contains("crane"));
    }

    #[test]
    fn test_guess_request_deserialization() {
        let json = r#"{"puzzle_date_iso_day": "2026-01-15", "word_guessed": "crane"}"#;
        let req: GuessRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.puzzle_date_iso_day,
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()
        );
        assert_eq!(req.word_guessed, "crane");
    }

    #[test]
    fn test_letter_grade_serialization() {
        assert_eq!(
            serde_json::to_string(&LetterGrade::Correct).unwrap(),
            "\"correct\""
        );
        assert_eq!(
            serde_json::to_string(&LetterGrade::Contained).unwrap(),
            "\"contained\""
        );
        assert_eq!(
            serde_json::to_string(&LetterGrade::Wrong).unwrap(),
            "\"wrong\""
        );
    }

    #[test]
    fn test_graded_letter() {
        let gl = GradedLetter::new('c', LetterGrade::Correct);
        let json = serde_json::to_string(&gl).unwrap();
        assert!(json.contains("\"letter\":\"c\""));
        assert!(json.contains("\"grade\":\"correct\""));
    }

    #[test]
    fn test_graded_game() {
        let game_id = Uuid::new_v4();
        let moves = vec![vec![
            GradedLetter::new('c', LetterGrade::Correct),
            GradedLetter::new('r', LetterGrade::Wrong),
            GradedLetter::new('a', LetterGrade::Contained),
            GradedLetter::new('n', LetterGrade::Wrong),
            GradedLetter::new('e', LetterGrade::Correct),
        ]];
        let game = GradedGame::new(game_id, "auth0|123", moves, false, None);
        assert_eq!(game.metadata.moves_qty, 1);
        assert!(!game.metadata.won);
        assert!(game.answer.is_none());
    }

    #[test]
    fn test_game_state_new() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let state = GameState::new("auth0|123", date);
        assert_eq!(state.user_id, "auth0|123");
        assert_eq!(state.puzzle_date, date);
        assert!(state.guesses.is_empty());
        assert!(!state.won);
        assert!(state.is_in_progress());
    }

    #[test]
    fn test_game_state_add_guess() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let mut state = GameState::new("auth0|123", date);
        state.add_guess("crane");
        state.add_guess("slate");
        assert_eq!(state.guess_count(), 2);
        assert_eq!(state.guesses, vec!["crane", "slate"]);
    }

    #[test]
    fn test_game_state_mark_won() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let mut state = GameState::new("auth0|123", date);
        state.add_guess("crane");
        state.mark_won();
        assert!(state.won);
        assert!(!state.is_in_progress());
    }

    #[test]
    fn test_game_state_max_guesses() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let mut state = GameState::new("auth0|123", date);
        for i in 0..6 {
            state.add_guess(format!("word{}", i));
        }
        assert_eq!(state.guess_count(), 6);
        assert!(!state.is_in_progress()); // No longer in progress after 6 guesses
    }

    #[test]
    fn test_game_state_pk() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let state = GameState::new("auth0|123", date);
        assert_eq!(state.pk(), "USER#auth0|123#PUZZLE#2026-01-15");
    }

    #[test]
    fn test_puzzle_answer_pk() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(PuzzleAnswer::pk(date), "PUZZLE#2026-01-15");
    }
}
