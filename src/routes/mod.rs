//! Route handlers for the scorekeeper API.

pub mod health;
pub mod scores;

pub use health::*;
pub use scores::{create_scores, list_scores};
