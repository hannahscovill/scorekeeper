//! Alert routing service for sending alerts to observability systems.
//!
//! This module provides a flexible, extensible alert routing system that follows
//! the same patterns established by the health check and secrets modules.
//!
//! # Architecture
//!
//! The alert system is built around the `AlertSender` trait, which allows for
//! easy extension with new alert destinations (Grafana OnCall, PagerDuty, Slack,
//! OpsGenie, etc.). Each sender is responsible for formatting and delivering
//! alerts to a single destination.
//!
//! The `AlertRouter` aggregates multiple senders and provides:
//! - Fan-out to multiple destinations
//! - Deduplication to prevent alert storms
//! - Retry with exponential backoff for transient failures
//! - Observability via tracing
//!
//! # Example
//!
//! ```ignore
//! use scorekeeper::services::alerts::{AlertRouter, GrafanaOnCallSender};
//! use scorekeeper::models::alert::Alert;
//!
//! let router = AlertRouter::new()
//!     .with_sender(Box::new(GrafanaOnCallSender::new(
//!         "grafana",
//!         "https://oncall.example.com/webhook/abc123",
//!     )))
//!     .with_dedup_window(Duration::from_secs(300));
//!
//! let alert = Alert::critical("Database connection lost", "Primary DB unreachable");
//! router.route_alert(&alert).await?;
//! ```

mod router;
mod sender;

pub use router::{AlertRouter, AlertRouterConfig, AlertRouterError, RouteResult};
pub use sender::{AlertSender, AlertSenderError, GrafanaOnCallSender};

#[cfg(test)]
pub use sender::MockAlertSender;
