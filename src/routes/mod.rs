//! Route handlers for the scorekeeper API.

pub mod games;
pub mod health;

pub use games::{create_games, get_games, list_games};
pub use health::*;
