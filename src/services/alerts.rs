//! Alert routing service for sending alerts to observability systems.

use crate::models::alert::{Alert, GrafanaOnCallPayload};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, error, info};

/// Configuration for an alert destination.
#[derive(Debug, Clone)]
pub struct AlertDestination {
    /// Name of the destination.
    pub name: String,
    /// Webhook URL to send alerts to.
    pub webhook_url: String,
    /// Whether this destination is enabled.
    pub enabled: bool,
}

impl AlertDestination {
    /// Creates a new alert destination.
    pub fn new(name: impl Into<String>, webhook_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            webhook_url: webhook_url.into(),
            enabled: true,
        }
    }

    /// Disables this destination.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Tracks when an alert was last sent for deduplication.
#[derive(Debug, Clone)]
struct AlertTracker {
    /// Last time this alert was sent.
    last_sent: DateTime<Utc>,
    /// Number of times this alert has been suppressed.
    suppressed_count: u64,
}

/// Alert routing service responsible for sending alerts to configured destinations.
pub struct AlertRouter {
    /// Configured alert destinations.
    destinations: Vec<AlertDestination>,
    /// HTTP client for sending webhooks.
    client: reqwest::Client,
    /// Deduplication window in seconds.
    dedup_window_secs: i64,
    /// Track recently sent alerts for deduplication.
    recent_alerts: Arc<DashMap<String, AlertTracker>>,
}

impl AlertRouter {
    /// Creates a new alert router with the given destinations.
    pub fn new(destinations: Vec<AlertDestination>) -> Self {
        Self {
            destinations,
            client: reqwest::Client::new(),
            dedup_window_secs: 300, // 5 minutes default
            recent_alerts: Arc::new(DashMap::new()),
        }
    }

    /// Sets the deduplication window in seconds.
    pub fn with_dedup_window(mut self, seconds: i64) -> Self {
        self.dedup_window_secs = seconds;
        self
    }

    /// Routes an alert to all enabled destinations.
    ///
    /// Returns the number of destinations that successfully received the alert.
    pub async fn route_alert(&self, alert: &Alert) -> Result<usize, AlertRouterError> {
        let dedup_key = alert.dedup_key();

        // Check if we should deduplicate this alert
        if self.should_deduplicate(&dedup_key) {
            debug!(
                alert_id = %alert.id,
                dedup_key = %dedup_key,
                "Alert deduplicated - sent recently"
            );

            // Increment suppressed count
            if let Some(mut tracker) = self.recent_alerts.get_mut(&dedup_key) {
                tracker.suppressed_count += 1;
            }

            return Ok(0); // Alert was deduplicated
        }

        // Send to all enabled destinations
        let mut success_count = 0;
        let mut last_error = None;

        for destination in &self.destinations {
            if !destination.enabled {
                debug!(destination = %destination.name, "Skipping disabled destination");
                continue;
            }

            match self.send_to_destination(alert, destination).await {
                Ok(_) => {
                    success_count += 1;
                    info!(
                        destination = %destination.name,
                        alert_id = %alert.id,
                        severity = %alert.severity,
                        title = %alert.title,
                        "Alert sent successfully"
                    );
                }
                Err(e) => {
                    error!(
                        destination = %destination.name,
                        alert_id = %alert.id,
                        error = %e,
                        "Failed to send alert to destination"
                    );
                    last_error = Some(e);
                }
            }
        }

        // Update deduplication tracker
        self.recent_alerts.insert(
            dedup_key,
            AlertTracker {
                last_sent: Utc::now(),
                suppressed_count: 0,
            },
        );

        // Clean up old entries periodically
        self.cleanup_old_trackers();

        // Return error if all destinations failed
        if success_count == 0 {
            if let Some(err) = last_error {
                return Err(err);
            }
        }

        Ok(success_count)
    }

    /// Checks if an alert should be deduplicated based on recent sends.
    fn should_deduplicate(&self, dedup_key: &str) -> bool {
        if let Some(tracker) = self.recent_alerts.get(dedup_key) {
            let elapsed = Utc::now()
                .signed_duration_since(tracker.last_sent)
                .num_seconds();

            if elapsed < self.dedup_window_secs {
                return true;
            }
        }
        false
    }

    /// Sends an alert to a specific destination.
    async fn send_to_destination(
        &self,
        alert: &Alert,
        destination: &AlertDestination,
    ) -> Result<(), AlertRouterError> {
        // Convert alert to Grafana OnCall format
        let payload: GrafanaOnCallPayload = alert.into();

        // Send webhook
        let response = self
            .client
            .post(&destination.webhook_url)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(AlertRouterError::RequestFailed)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());

            return Err(AlertRouterError::WebhookFailed {
                status: status.as_u16(),
                body,
            });
        }

        Ok(())
    }

    /// Removes old entries from the deduplication tracker.
    fn cleanup_old_trackers(&self) {
        let cutoff = Utc::now() - Duration::seconds(self.dedup_window_secs * 2);

        self.recent_alerts
            .retain(|_, tracker| tracker.last_sent > cutoff);
    }

    /// Returns statistics about suppressed alerts.
    pub fn get_suppression_stats(&self) -> Vec<(String, u64)> {
        self.recent_alerts
            .iter()
            .filter(|entry| entry.value().suppressed_count > 0)
            .map(|entry| (entry.key().clone(), entry.value().suppressed_count))
            .collect()
    }
}

/// Errors that can occur during alert routing.
#[derive(Debug, thiserror::Error)]
pub enum AlertRouterError {
    /// Failed to send HTTP request.
    #[error("Failed to send request: {0}")]
    RequestFailed(#[source] reqwest::Error),

    /// Webhook endpoint returned an error.
    #[error("Webhook failed with status {status}: {body}")]
    WebhookFailed { status: u16, body: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alert::Alert;

    #[test]
    fn test_alert_destination_new() {
        let dest = AlertDestination::new("grafana", "https://example.com/webhook");
        assert_eq!(dest.name, "grafana");
        assert_eq!(dest.webhook_url, "https://example.com/webhook");
        assert!(dest.enabled);
    }

    #[test]
    fn test_alert_destination_disabled() {
        let dest = AlertDestination::new("grafana", "https://example.com/webhook").disabled();
        assert!(!dest.enabled);
    }

    #[test]
    fn test_alert_router_new() {
        let destinations = vec![AlertDestination::new("test", "https://example.com")];
        let router = AlertRouter::new(destinations);
        assert_eq!(router.destinations.len(), 1);
        assert_eq!(router.dedup_window_secs, 300);
    }

    #[test]
    fn test_alert_router_with_dedup_window() {
        let router = AlertRouter::new(vec![]).with_dedup_window(600);
        assert_eq!(router.dedup_window_secs, 600);
    }

    #[test]
    fn test_should_deduplicate() {
        let router = AlertRouter::new(vec![]).with_dedup_window(60);
        let dedup_key = "test:critical:Test Alert";

        // First check should not deduplicate
        assert!(!router.should_deduplicate(dedup_key));

        // Add a recent tracker
        router.recent_alerts.insert(
            dedup_key.to_string(),
            AlertTracker {
                last_sent: Utc::now(),
                suppressed_count: 0,
            },
        );

        // Should deduplicate now
        assert!(router.should_deduplicate(dedup_key));

        // Add an old tracker
        let old_time = Utc::now() - Duration::seconds(120);
        router.recent_alerts.insert(
            "old_key".to_string(),
            AlertTracker {
                last_sent: old_time,
                suppressed_count: 0,
            },
        );

        // Should not deduplicate old tracker
        assert!(!router.should_deduplicate("old_key"));
    }

    #[test]
    fn test_cleanup_old_trackers() {
        let router = AlertRouter::new(vec![]).with_dedup_window(60);

        // Add recent tracker
        router.recent_alerts.insert(
            "recent".to_string(),
            AlertTracker {
                last_sent: Utc::now(),
                suppressed_count: 0,
            },
        );

        // Add old tracker
        let old_time = Utc::now() - Duration::seconds(200);
        router.recent_alerts.insert(
            "old".to_string(),
            AlertTracker {
                last_sent: old_time,
                suppressed_count: 0,
            },
        );

        assert_eq!(router.recent_alerts.len(), 2);

        // Cleanup should remove old tracker
        router.cleanup_old_trackers();

        assert_eq!(router.recent_alerts.len(), 1);
        assert!(router.recent_alerts.contains_key("recent"));
        assert!(!router.recent_alerts.contains_key("old"));
    }

    #[test]
    fn test_get_suppression_stats() {
        let router = AlertRouter::new(vec![]);

        router.recent_alerts.insert(
            "key1".to_string(),
            AlertTracker {
                last_sent: Utc::now(),
                suppressed_count: 5,
            },
        );

        router.recent_alerts.insert(
            "key2".to_string(),
            AlertTracker {
                last_sent: Utc::now(),
                suppressed_count: 0,
            },
        );

        router.recent_alerts.insert(
            "key3".to_string(),
            AlertTracker {
                last_sent: Utc::now(),
                suppressed_count: 3,
            },
        );

        let stats = router.get_suppression_stats();
        assert_eq!(stats.len(), 2); // Only non-zero suppressions

        let total_suppressed: u64 = stats.iter().map(|(_, count)| count).sum();
        assert_eq!(total_suppressed, 8);
    }

    #[tokio::test]
    async fn test_route_alert_deduplication() {
        let router = AlertRouter::new(vec![]).with_dedup_window(60);

        let alert = Alert::critical("Test Alert", "Test message");

        // First send should succeed (but with 0 destinations)
        let result = router.route_alert(&alert).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        // Immediate second send should be deduplicated
        let result = router.route_alert(&alert).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        // Check suppression count increased
        let dedup_key = alert.dedup_key();
        let suppressed_count = router
            .recent_alerts
            .get(&dedup_key)
            .map(|tracker| tracker.suppressed_count);
        assert_eq!(suppressed_count, Some(1), "Tracker should exist with suppressed_count=1");
    }
}
