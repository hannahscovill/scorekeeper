//! Data structures for the scorekeeper API.

pub mod error;
pub mod game;
pub mod guess;

pub use error::{AppError, ErrorBody, ErrorResponse, ValidationDetail};
pub use game::*;
pub use guess::*;
