//! Alert models and types for routing to observability systems.
//!
//! This module defines the core data structures for the alert routing system.
//! Alerts are domain events that should be delivered to external observability
//! systems like Grafana OnCall, PagerDuty, or Slack.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Alert severity levels.
///
/// Severity determines routing behavior and visual presentation in
/// observability systems. Use these consistently across the application:
///
/// - **Critical**: Requires immediate human intervention (pager-worthy)
/// - **Warning**: Should be addressed soon but not an emergency
/// - **Info**: Informational messages for awareness (deployments, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    /// Critical issues requiring immediate attention.
    Critical,
    /// Warning conditions that should be addressed soon.
    Warning,
    /// Informational messages for awareness.
    Info,
}

impl AlertSeverity {
    /// Returns the string representation of the severity level.
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertSeverity::Critical => "critical",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Info => "info",
        }
    }

    /// Returns the numeric priority (lower is more severe).
    ///
    /// Useful for sorting or filtering by severity.
    pub fn priority(&self) -> u8 {
        match self {
            AlertSeverity::Critical => 1,
            AlertSeverity::Warning => 2,
            AlertSeverity::Info => 3,
        }
    }
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An alert to be routed to observability systems.
///
/// Alerts are immutable after creation. Use the builder pattern to
/// construct alerts with the appropriate metadata.
///
/// # Example
///
/// ```
/// use scorekeeper::models::alert::{Alert, AlertSeverity};
///
/// let alert = Alert::critical("Database connection lost", "Primary DB at db.example.com is unreachable")
///     .with_source("scorekeeper-api")
///     .with_label("region", "us-west-2")
///     .with_label("env", "production");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique identifier for the alert.
    pub id: String,
    /// Alert severity level.
    pub severity: AlertSeverity,
    /// Alert title/summary (should be brief and actionable).
    pub title: String,
    /// Detailed alert message with context for responders.
    pub message: String,
    /// Source of the alert (e.g., "scorekeeper-api", "worker-process").
    pub source: String,
    /// Additional metadata/labels for filtering and routing.
    #[serde(default)]
    pub labels: HashMap<String, String>,
    /// Timestamp when the alert was created.
    pub timestamp: DateTime<Utc>,
}

impl Alert {
    /// Creates a new alert with the given parameters.
    pub fn new(
        severity: AlertSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            severity,
            title: title.into(),
            message: message.into(),
            source: "scorekeeper".to_string(),
            labels: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    /// Creates a critical alert.
    pub fn critical(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertSeverity::Critical, title, message)
    }

    /// Creates a warning alert.
    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertSeverity::Warning, title, message)
    }

    /// Creates an info alert.
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AlertSeverity::Info, title, message)
    }

    /// Adds a label to the alert.
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Sets the source of the alert.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Generates a deduplication key for this alert.
    ///
    /// Alerts with the same dedup key are considered duplicates and may be
    /// suppressed by the alert router. The key is based on source, severity,
    /// and title - not the message content or timestamp.
    pub fn dedup_key(&self) -> String {
        format!("{}:{}:{}", self.source, self.severity, self.title)
    }
}

/// Grafana OnCall webhook payload format.
///
/// This structure matches the expected format for Grafana OnCall webhook
/// integrations. Custom fields are flattened into the JSON payload.
///
/// See: https://grafana.com/docs/oncall/latest/integrations/webhook/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrafanaOnCallPayload {
    /// Alert title.
    pub title: String,
    /// Alert message/description.
    pub message: String,
    /// Alert state: "alerting" or "ok".
    pub state: String,
    /// Alert severity level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// Deduplication key for grouping related alerts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedup_key: Option<String>,
    /// Additional custom fields.
    #[serde(flatten)]
    pub custom_fields: HashMap<String, serde_json::Value>,
}

impl From<&Alert> for GrafanaOnCallPayload {
    fn from(alert: &Alert) -> Self {
        let mut custom_fields = HashMap::new();
        custom_fields.insert("alert_id".to_string(), serde_json::json!(alert.id));
        custom_fields.insert("source".to_string(), serde_json::json!(alert.source));
        custom_fields.insert(
            "timestamp".to_string(),
            serde_json::json!(alert.timestamp.to_rfc3339()),
        );

        // Add alert labels as custom fields
        for (key, value) in &alert.labels {
            custom_fields.insert(key.clone(), serde_json::json!(value));
        }

        Self {
            title: alert.title.clone(),
            message: alert.message.clone(),
            state: "alerting".to_string(),
            severity: Some(alert.severity.as_str().to_string()),
            dedup_key: Some(alert.dedup_key()),
            custom_fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity_as_str() {
        assert_eq!(AlertSeverity::Critical.as_str(), "critical");
        assert_eq!(AlertSeverity::Warning.as_str(), "warning");
        assert_eq!(AlertSeverity::Info.as_str(), "info");
    }

    #[test]
    fn test_alert_severity_display() {
        assert_eq!(AlertSeverity::Critical.to_string(), "critical");
        assert_eq!(AlertSeverity::Warning.to_string(), "warning");
        assert_eq!(AlertSeverity::Info.to_string(), "info");
    }

    #[test]
    fn test_alert_severity_priority() {
        assert!(AlertSeverity::Critical.priority() < AlertSeverity::Warning.priority());
        assert!(AlertSeverity::Warning.priority() < AlertSeverity::Info.priority());
    }

    #[test]
    fn test_alert_new() {
        let alert = Alert::new(AlertSeverity::Warning, "Test Alert", "Test message");
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert_eq!(alert.title, "Test Alert");
        assert_eq!(alert.message, "Test message");
        assert_eq!(alert.source, "scorekeeper");
        assert!(!alert.id.is_empty());
    }

    #[test]
    fn test_alert_critical() {
        let alert = Alert::critical("Critical Issue", "System is down");
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert_eq!(alert.title, "Critical Issue");
        assert_eq!(alert.message, "System is down");
    }

    #[test]
    fn test_alert_warning() {
        let alert = Alert::warning("High Memory", "Memory usage is high");
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert_eq!(alert.title, "High Memory");
    }

    #[test]
    fn test_alert_info() {
        let alert = Alert::info("Deployment", "New version deployed");
        assert_eq!(alert.severity, AlertSeverity::Info);
        assert_eq!(alert.title, "Deployment");
    }

    #[test]
    fn test_alert_with_label() {
        let alert = Alert::critical("Test", "Message")
            .with_label("env", "production")
            .with_label("region", "us-west-2");

        assert_eq!(alert.labels.get("env"), Some(&"production".to_string()));
        assert_eq!(alert.labels.get("region"), Some(&"us-west-2".to_string()));
    }

    #[test]
    fn test_alert_with_source() {
        let alert = Alert::critical("Test", "Message").with_source("api-gateway");
        assert_eq!(alert.source, "api-gateway");
    }

    #[test]
    fn test_alert_dedup_key() {
        let alert1 = Alert::critical("Database Error", "Connection lost");
        let alert2 = Alert::critical("Database Error", "Connection lost");
        let alert3 = Alert::warning("Database Error", "Connection lost");

        // Same severity and title should have same dedup key
        assert_eq!(alert1.dedup_key(), alert2.dedup_key());
        // Different severity should have different dedup key
        assert_ne!(alert1.dedup_key(), alert3.dedup_key());
    }

    #[test]
    fn test_grafana_oncall_payload_conversion() {
        let alert = Alert::critical("Test Alert", "Test message")
            .with_label("env", "production")
            .with_source("test-service");

        let payload: GrafanaOnCallPayload = (&alert).into();

        assert_eq!(payload.title, "Test Alert");
        assert_eq!(payload.message, "Test message");
        assert_eq!(payload.state, "alerting");
        assert_eq!(payload.severity, Some("critical".to_string()));
        assert!(payload.dedup_key.is_some());
        assert_eq!(
            payload.custom_fields.get("source"),
            Some(&serde_json::json!("test-service"))
        );
        assert_eq!(
            payload.custom_fields.get("env"),
            Some(&serde_json::json!("production"))
        );
    }

    #[test]
    fn test_alert_serialization() {
        let alert = Alert::warning("Test", "Message").with_label("key", "value");
        let json = serde_json::to_string(&alert).unwrap();
        let deserialized: Alert = serde_json::from_str(&json).unwrap();

        assert_eq!(alert.severity, deserialized.severity);
        assert_eq!(alert.title, deserialized.title);
        assert_eq!(alert.message, deserialized.message);
        assert_eq!(alert.labels, deserialized.labels);
    }

    #[test]
    fn test_grafana_payload_serialization() {
        let alert = Alert::critical("Test", "Message");
        let payload: GrafanaOnCallPayload = (&alert).into();
        let json = serde_json::to_string(&payload).unwrap();

        assert!(json.contains("\"title\""));
        assert!(json.contains("\"message\""));
        assert!(json.contains("\"state\""));
        assert!(json.contains("\"severity\""));
        assert!(json.contains("\"dedup_key\""));
    }
}
