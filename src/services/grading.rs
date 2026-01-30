//! Grading logic for word guessing games.

use crate::models::guess::{GradedGuess, GradedLetter, LetterGrade};

/// Grades a guess against the answer word.
///
/// Returns a vector of GradedLetter for each position:
/// - Correct: letter is in correct position
/// - Contained: letter is in word but wrong position
/// - Wrong: letter is not in word
///
/// This implements standard Wordle grading rules:
/// - Green (Correct) takes priority
/// - Yellow (Contained) only shows for remaining unmatched letters
pub fn grade_guess(guess: &str, answer: &str) -> GradedGuess {
    let guess_chars: Vec<char> = guess.chars().collect();
    let answer_chars: Vec<char> = answer.chars().collect();

    // Track which answer letters are still available for "contained" matches
    let mut available: Vec<Option<char>> = answer_chars.iter().copied().map(Some).collect();
    let mut grades: Vec<LetterGrade> = vec![LetterGrade::Wrong; 5];

    // First pass: mark exact matches (Correct)
    for i in 0..5 {
        if guess_chars[i] == answer_chars[i] {
            grades[i] = LetterGrade::Correct;
            available[i] = None; // This letter is used
        }
    }

    // Second pass: mark contained letters (yellow)
    for i in 0..5 {
        if grades[i] == LetterGrade::Correct {
            continue; // Already matched
        }

        // Look for this letter in remaining available positions
        if let Some(pos) = available.iter().position(|&c| c == Some(guess_chars[i])) {
            grades[i] = LetterGrade::Contained;
            available[pos] = None; // Use this letter
        }
    }

    // Build the result
    guess_chars
        .into_iter()
        .zip(grades)
        .map(|(letter, grade)| GradedLetter::new(letter, grade))
        .collect()
}

/// Checks if a graded guess is a winning guess (all correct).
pub fn is_winning_guess(graded: &GradedGuess) -> bool {
    graded.iter().all(|g| g.grade == LetterGrade::Correct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grade_all_correct() {
        let graded = grade_guess("crane", "crane");
        assert!(graded.iter().all(|g| g.grade == LetterGrade::Correct));
        assert!(is_winning_guess(&graded));
    }

    #[test]
    fn test_grade_all_wrong() {
        let graded = grade_guess("xxxxx", "crane");
        assert!(graded.iter().all(|g| g.grade == LetterGrade::Wrong));
        assert!(!is_winning_guess(&graded));
    }

    #[test]
    fn test_grade_mixed() {
        // guess "slate" against answer "stale"
        // s - correct (position 0)
        // l - contained (in word, but position 2 vs 3)
        // a - contained (in word, but position 2 vs 2) - wait, 'a' is at position 2 in both!
        // Actually: s=0, l=1, a=2, t=3, e=4 (slate)
        //           s=0, t=1, a=2, l=3, e=4 (stale)
        // s - correct
        // l - contained (l is at position 3 in answer)
        // a - correct
        // t - contained (t is at position 1 in answer)
        // e - correct
        let graded = grade_guess("slate", "stale");
        assert_eq!(graded[0].grade, LetterGrade::Correct); // s
        assert_eq!(graded[1].grade, LetterGrade::Contained); // l
        assert_eq!(graded[2].grade, LetterGrade::Correct); // a
        assert_eq!(graded[3].grade, LetterGrade::Contained); // t
        assert_eq!(graded[4].grade, LetterGrade::Correct); // e
    }

    #[test]
    fn test_grade_double_letter_in_guess() {
        // guess "hello" against answer "helps"
        // h - correct
        // e - correct
        // l - correct
        // l - wrong (only one 'l' in answer, already used)
        // o - wrong
        let graded = grade_guess("hello", "helps");
        assert_eq!(graded[0].grade, LetterGrade::Correct); // h
        assert_eq!(graded[1].grade, LetterGrade::Correct); // e
        assert_eq!(graded[2].grade, LetterGrade::Correct); // l
        assert_eq!(graded[3].grade, LetterGrade::Wrong); // l (duplicate)
        assert_eq!(graded[4].grade, LetterGrade::Wrong); // o
    }

    #[test]
    fn test_grade_double_letter_in_answer() {
        // guess "crane" against answer "creep"
        // c - correct
        // r - correct
        // a - wrong
        // n - wrong
        // e - contained (e is at position 2 and 3 in answer)
        let graded = grade_guess("crane", "creep");
        assert_eq!(graded[0].grade, LetterGrade::Correct); // c
        assert_eq!(graded[1].grade, LetterGrade::Correct); // r
        assert_eq!(graded[2].grade, LetterGrade::Wrong); // a
        assert_eq!(graded[3].grade, LetterGrade::Wrong); // n
        assert_eq!(graded[4].grade, LetterGrade::Contained); // e
    }

    #[test]
    fn test_grade_correct_takes_priority() {
        // guess "geese" against answer "creep"
        // g - wrong
        // e - contained (answer has e at 2,3 but position 1 is 'r')
        // e - correct (exact match at position 2)
        // s - wrong
        // e - wrong (both e's in answer already used)
        let graded = grade_guess("geese", "creep");
        assert_eq!(graded[0].grade, LetterGrade::Wrong); // g
        assert_eq!(graded[1].grade, LetterGrade::Contained); // e - uses answer position 3
        assert_eq!(graded[2].grade, LetterGrade::Correct); // e - exact match at 2
        assert_eq!(graded[3].grade, LetterGrade::Wrong); // s
        assert_eq!(graded[4].grade, LetterGrade::Wrong); // e - no more e's available
    }

    #[test]
    fn test_grade_letters() {
        let graded = grade_guess("crane", "stale");
        assert_eq!(graded[0].letter, 'c');
        assert_eq!(graded[1].letter, 'r');
        assert_eq!(graded[2].letter, 'a');
        assert_eq!(graded[3].letter, 'n');
        assert_eq!(graded[4].letter, 'e');
    }
}
