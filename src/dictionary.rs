//! Word dictionary for validating guesses.
//!
//! Uses a compile-time perfect hash set generated from `data/words.txt`.

// Include the generated phf::Set
include!(concat!(env!("OUT_DIR"), "/dictionary.rs"));

/// Checks if a word is a valid 5-letter word in the dictionary.
pub fn is_valid_word(word: &str) -> bool {
    VALID_WORDS.contains(word.to_lowercase().as_str())
}

/// Returns the total number of valid words in the dictionary.
pub fn word_count() -> usize {
    VALID_WORDS.len()
}

/// Returns an iterator over all valid words in the dictionary.
pub fn all_words() -> impl Iterator<Item = &'static str> {
    ALL_WORDS.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_word() {
        assert!(is_valid_word("crane"));
        assert!(is_valid_word("CRANE")); // case insensitive
        assert!(is_valid_word("Crane"));
    }

    #[test]
    fn test_invalid_word() {
        assert!(!is_valid_word("zzzzz"));
        assert!(!is_valid_word("notaword"));
        assert!(!is_valid_word("ab")); // too short
    }

    #[test]
    fn test_word_count() {
        // Should have loaded words from data/words.txt
        assert!(word_count() > 0);
    }
}
