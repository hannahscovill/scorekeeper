//! Route handlers for the scorekeeper API.

pub mod game;
pub mod games;
pub mod guess;
pub mod health;
pub mod history;
pub mod issues;
pub mod profile;
pub mod puzzle;

pub use game::get_game;
pub use games::{create_games, get_games, list_games};
pub use guess::submit_guess;
pub use health::*;
pub use history::get_history;
pub use issues::create_issue;
pub use profile::{get_profile, revert_avatar, update_profile, upload_avatar};
pub use puzzle::{clear_puzzle_cache, get_puzzle_by_date, get_puzzles, set_puzzle};
