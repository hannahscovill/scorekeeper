//! Data structures for the scorekeeper API.

pub mod alert;
pub mod error;
pub mod score;

pub use alert::{Alert, AlertSeverity, GrafanaOnCallPayload};
pub use error::{AppError, ErrorBody, ErrorResponse, ValidationDetail};
pub use score::*;
