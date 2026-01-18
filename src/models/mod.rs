//! Data structures for the scorekeeper API.

pub mod error;
pub mod score;

pub use error::{AppError, ErrorBody, ErrorResponse, ValidationDetail};
pub use score::*;
