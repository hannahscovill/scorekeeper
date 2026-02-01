//! Route handlers for the scorekeeper API.

pub mod games;
pub mod guess;
pub mod health;
pub mod history;
pub mod profile;
pub mod puzzle;

pub use games::{create_games, get_games, list_games};
pub use guess::submit_guess;
pub use health::*;
pub use history::get_history;
pub use profile::{get_profile, update_profile, upload_avatar};
pub use puzzle::set_puzzle;
