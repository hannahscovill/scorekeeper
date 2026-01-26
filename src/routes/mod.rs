//! Route handlers for the scorekeeper API.

pub mod games;
pub mod health;
pub mod profile;

pub use games::{create_games, get_games, list_games};
pub use health::*;
pub use profile::{get_profile, update_profile};
