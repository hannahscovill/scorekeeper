//! Route handlers for the scorekeeper API.

pub mod health;
pub mod scores;

pub use health::{deep_health_check, health_check};
pub use scores::{create_scores, get_scores, list_scores};
