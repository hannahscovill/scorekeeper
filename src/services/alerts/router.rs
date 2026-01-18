//! Alert router for fan-out delivery with deduplication and retry.

use super::sender::{AlertSender, AlertSenderError};
use crate::models::alert::Alert;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Configuration for the alert router.
#[derive(Debug, Clone)]
pub struct AlertRouterConfig {
    /// Deduplication window - alerts with the same dedup key within this window are suppressed.
    pub dedup_window: Duration,
    /// Maximum retry attempts for transient failures.
    pub max_retries: u32,
    /// Base delay for exponential backoff (doubles with each retry).
    pub retry_base_delay: Duration,
    /// Maximum delay between retries.
    pub retry_max_delay: Duration,
}

impl Default for AlertRouterConfig {
    fn default() -> Self {
        Self {
            dedup_window: Duration::from_secs(300), // 5 minutes
            max_retries: 3,
            retry_base_delay: Duration::from_millis(100),
            retry_max_delay: Duration::from_secs(10),
        }
    }
}

impl AlertRouterConfig {
    /// Creates a new configuration with the specified deduplication window.
    pub fn with_dedup_window(mut self, window: Duration) -> Self {
        self.dedup_window = window;
        self
    }

    /// Sets the maximum retry attempts.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Disables retries.
    pub fn no_retries(mut self) -> Self {
        self.max_retries = 0;
        self
    }
}

/// Result of routing an alert.
#[derive(Debug)]
pub struct RouteResult {
    /// Number of senders that successfully delivered the alert.
    pub successes: usize,
    /// Number of senders that failed to deliver.
    pub failures: usize,
    /// Whether the alert was deduplicated (not sent).
    pub deduplicated: bool,
    /// Names of senders that failed.
    pub failed_senders: Vec<String>,
}

impl RouteResult {
    /// Returns true if all enabled senders succeeded.
    pub fn is_complete_success(&self) -> bool {
        self.failures == 0 && !self.deduplicated
    }

    /// Returns true if at least one sender succeeded.
    pub fn is_partial_success(&self) -> bool {
        self.successes > 0
    }
}

/// Errors that can occur during alert routing.
#[derive(Debug, Error)]
pub enum AlertRouterError {
    /// All senders failed to deliver the alert.
    #[error("All senders failed: {0}")]
    AllSendersFailed(String),

    /// No senders are configured or enabled.
    #[error("No senders configured or enabled")]
    NoSenders,
}

/// Tracks alert deduplication state.
struct DedupTracker {
    last_sent: DateTime<Utc>,
    suppressed_count: AtomicU64,
}

/// Alert router with fan-out, deduplication, and retry capabilities.
///
/// The router maintains a collection of `AlertSender` implementations and
/// handles the complexity of delivering alerts reliably:
///
/// - **Fan-out**: Sends to all enabled senders
/// - **Deduplication**: Prevents alert storms by suppressing duplicates
/// - **Retry**: Automatically retries transient failures with backoff
/// - **Observability**: Emits tracing events for monitoring
pub struct AlertRouter {
    senders: Vec<Box<dyn AlertSender>>,
    config: AlertRouterConfig,
    dedup_state: Arc<DashMap<String, DedupTracker>>,
}

impl AlertRouter {
    /// Creates a new alert router with default configuration.
    pub fn new() -> Self {
        Self {
            senders: Vec::new(),
            config: AlertRouterConfig::default(),
            dedup_state: Arc::new(DashMap::new()),
        }
    }

    /// Creates a new alert router with the specified configuration.
    pub fn with_config(config: AlertRouterConfig) -> Self {
        Self {
            senders: Vec::new(),
            config,
            dedup_state: Arc::new(DashMap::new()),
        }
    }

    /// Adds an alert sender to the router.
    pub fn with_sender(mut self, sender: Box<dyn AlertSender>) -> Self {
        self.senders.push(sender);
        self
    }

    /// Sets the deduplication window.
    pub fn with_dedup_window(mut self, window: Duration) -> Self {
        self.config.dedup_window = window;
        self
    }

    /// Routes an alert to all enabled senders.
    ///
    /// # Deduplication
    ///
    /// Alerts are deduplicated based on their `dedup_key()`. If an alert with
    /// the same key was sent within the deduplication window, this alert will
    /// be suppressed and the method returns immediately with `deduplicated: true`.
    ///
    /// # Retry Behavior
    ///
    /// For senders that fail with a retryable error, the router will automatically
    /// retry with exponential backoff up to `max_retries` times.
    ///
    /// # Returns
    ///
    /// A `RouteResult` indicating success/failure counts and deduplication status.
    pub async fn route_alert(&self, alert: &Alert) -> Result<RouteResult, AlertRouterError> {
        let dedup_key = alert.dedup_key();

        // Check deduplication
        if self.should_deduplicate(&dedup_key) {
            debug!(
                alert_id = %alert.id,
                dedup_key = %dedup_key,
                "Alert deduplicated - sent within window"
            );

            // Increment suppressed count
            if let Some(tracker) = self.dedup_state.get(&dedup_key) {
                tracker.suppressed_count.fetch_add(1, Ordering::Relaxed);
            }

            return Ok(RouteResult {
                successes: 0,
                failures: 0,
                deduplicated: true,
                failed_senders: vec![],
            });
        }

        // Get enabled senders
        let enabled_senders: Vec<_> = self.senders.iter().filter(|s| s.is_enabled()).collect();

        if enabled_senders.is_empty() {
            warn!(alert_id = %alert.id, "No enabled senders configured");
            return Err(AlertRouterError::NoSenders);
        }

        // Send to all enabled senders
        let mut successes = 0;
        let mut failures = 0;
        let mut failed_senders = Vec::new();
        let mut last_error = None;

        for sender in enabled_senders {
            match self.send_with_retry(sender.as_ref(), alert).await {
                Ok(()) => {
                    successes += 1;
                    info!(
                        sender = %sender.name(),
                        alert_id = %alert.id,
                        severity = %alert.severity,
                        title = %alert.title,
                        "Alert delivered successfully"
                    );
                }
                Err(e) => {
                    failures += 1;
                    failed_senders.push(sender.name().to_string());
                    error!(
                        sender = %sender.name(),
                        alert_id = %alert.id,
                        error = %e,
                        "Failed to deliver alert after retries"
                    );
                    last_error = Some(e);
                }
            }
        }

        // Update deduplication state on any success
        if successes > 0 {
            self.dedup_state.insert(
                dedup_key,
                DedupTracker {
                    last_sent: Utc::now(),
                    suppressed_count: AtomicU64::new(0),
                },
            );

            // Periodic cleanup of old entries
            self.cleanup_old_entries();
        }

        // Return error if all senders failed
        if successes == 0 {
            if let Some(err) = last_error {
                return Err(AlertRouterError::AllSendersFailed(err.to_string()));
            }
        }

        Ok(RouteResult {
            successes,
            failures,
            deduplicated: false,
            failed_senders,
        })
    }

    /// Sends an alert with retry logic.
    async fn send_with_retry(
        &self,
        sender: &dyn AlertSender,
        alert: &Alert,
    ) -> Result<(), AlertSenderError> {
        let mut last_error = None;
        let mut delay = self.config.retry_base_delay;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                debug!(
                    sender = %sender.name(),
                    alert_id = %alert.id,
                    attempt = attempt,
                    delay_ms = %delay.as_millis(),
                    "Retrying alert delivery"
                );
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay * 2, self.config.retry_max_delay);
            }

            // Apply sender timeout
            let send_future = sender.send(alert);
            let result = tokio::time::timeout(sender.timeout(), send_future).await;

            match result {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) => {
                    if !e.is_retryable() || attempt == self.config.max_retries {
                        return Err(e);
                    }
                    last_error = Some(e);
                }
                Err(_timeout) => {
                    let err = AlertSenderError::Transient(format!(
                        "Timeout after {:?}",
                        sender.timeout()
                    ));
                    if attempt == self.config.max_retries {
                        return Err(err);
                    }
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AlertSenderError::Transient("Unknown error".into())))
    }

    /// Checks if an alert should be deduplicated.
    fn should_deduplicate(&self, dedup_key: &str) -> bool {
        if let Some(tracker) = self.dedup_state.get(dedup_key) {
            let elapsed = Utc::now()
                .signed_duration_since(tracker.last_sent)
                .num_milliseconds();

            if elapsed >= 0 && elapsed < self.config.dedup_window.as_millis() as i64 {
                return true;
            }
        }
        false
    }

    /// Removes stale entries from the deduplication state.
    fn cleanup_old_entries(&self) {
        let cutoff =
            Utc::now() - ChronoDuration::milliseconds(self.config.dedup_window.as_millis() as i64 * 2);

        self.dedup_state
            .retain(|_, tracker| tracker.last_sent > cutoff);
    }

    /// Returns statistics about suppressed alerts.
    ///
    /// Returns a list of (dedup_key, suppressed_count) pairs for alerts
    /// that have been suppressed at least once.
    pub fn suppression_stats(&self) -> Vec<(String, u64)> {
        self.dedup_state
            .iter()
            .filter_map(|entry| {
                let count = entry.value().suppressed_count.load(Ordering::Relaxed);
                if count > 0 {
                    Some((entry.key().clone(), count))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the number of configured senders.
    pub fn sender_count(&self) -> usize {
        self.senders.len()
    }

    /// Returns the number of enabled senders.
    pub fn enabled_sender_count(&self) -> usize {
        self.senders.iter().filter(|s| s.is_enabled()).count()
    }
}

impl Default for AlertRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alert::Alert;
    use crate::services::alerts::MockAlertSender;

    #[test]
    fn test_config_defaults() {
        let config = AlertRouterConfig::default();
        assert_eq!(config.dedup_window, Duration::from_secs(300));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_builder() {
        let config = AlertRouterConfig::default()
            .with_dedup_window(Duration::from_secs(60))
            .with_max_retries(5);

        assert_eq!(config.dedup_window, Duration::from_secs(60));
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_route_result() {
        let success = RouteResult {
            successes: 2,
            failures: 0,
            deduplicated: false,
            failed_senders: vec![],
        };
        assert!(success.is_complete_success());
        assert!(success.is_partial_success());

        let partial = RouteResult {
            successes: 1,
            failures: 1,
            deduplicated: false,
            failed_senders: vec!["slack".to_string()],
        };
        assert!(!partial.is_complete_success());
        assert!(partial.is_partial_success());

        let deduped = RouteResult {
            successes: 0,
            failures: 0,
            deduplicated: true,
            failed_senders: vec![],
        };
        assert!(!deduped.is_complete_success());
        assert!(!deduped.is_partial_success());
    }

    #[tokio::test]
    async fn test_route_alert_success() {
        let sender = MockAlertSender::new("test");
        let router = AlertRouter::new().with_sender(Box::new(sender));

        let alert = Alert::critical("Test", "Message");
        let result = router.route_alert(&alert).await.unwrap();

        assert_eq!(result.successes, 1);
        assert_eq!(result.failures, 0);
        assert!(!result.deduplicated);
    }

    #[tokio::test]
    async fn test_route_alert_multiple_senders() {
        let router = AlertRouter::new()
            .with_sender(Box::new(MockAlertSender::new("grafana")))
            .with_sender(Box::new(MockAlertSender::new("slack")));

        let alert = Alert::warning("Test", "Message");
        let result = router.route_alert(&alert).await.unwrap();

        assert_eq!(result.successes, 2);
        assert_eq!(result.failures, 0);
    }

    #[tokio::test]
    async fn test_route_alert_partial_failure() {
        let config = AlertRouterConfig::default().no_retries();
        let router = AlertRouter::with_config(config)
            .with_sender(Box::new(MockAlertSender::new("grafana")))
            .with_sender(Box::new(MockAlertSender::new("slack").failing()));

        let alert = Alert::warning("Test", "Message");
        let result = router.route_alert(&alert).await.unwrap();

        assert_eq!(result.successes, 1);
        assert_eq!(result.failures, 1);
        assert!(result.failed_senders.contains(&"slack".to_string()));
    }

    #[tokio::test]
    async fn test_route_alert_deduplication() {
        let router = AlertRouter::new()
            .with_sender(Box::new(MockAlertSender::new("test")))
            .with_dedup_window(Duration::from_secs(60));

        let alert = Alert::critical("Test Alert", "Same message");

        // First send should succeed
        let result1 = router.route_alert(&alert).await.unwrap();
        assert_eq!(result1.successes, 1);
        assert!(!result1.deduplicated);

        // Immediate second send should be deduplicated
        let result2 = router.route_alert(&alert).await.unwrap();
        assert_eq!(result2.successes, 0);
        assert!(result2.deduplicated);

        // Check suppression stats
        let stats = router.suppression_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].1, 1);
    }

    #[tokio::test]
    async fn test_route_alert_no_senders() {
        let router = AlertRouter::new();
        let alert = Alert::info("Test", "Message");

        let result = router.route_alert(&alert).await;
        assert!(matches!(result, Err(AlertRouterError::NoSenders)));
    }

    #[tokio::test]
    async fn test_route_alert_disabled_sender_skipped() {
        let router = AlertRouter::new()
            .with_sender(Box::new(MockAlertSender::new("enabled")))
            .with_sender(Box::new(MockAlertSender::new("disabled").disabled()));

        assert_eq!(router.sender_count(), 2);
        assert_eq!(router.enabled_sender_count(), 1);

        let alert = Alert::warning("Test", "Message");
        let result = router.route_alert(&alert).await.unwrap();

        assert_eq!(result.successes, 1);
    }

    #[tokio::test]
    async fn test_route_alert_all_fail() {
        let config = AlertRouterConfig::default().no_retries();
        let router = AlertRouter::with_config(config)
            .with_sender(Box::new(MockAlertSender::new("sender1").failing()))
            .with_sender(Box::new(MockAlertSender::new("sender2").failing()));

        let alert = Alert::critical("Test", "Message");
        let result = router.route_alert(&alert).await;

        assert!(matches!(result, Err(AlertRouterError::AllSendersFailed(_))));
    }

    #[tokio::test]
    async fn test_cleanup_old_entries() {
        let router = AlertRouter::new()
            .with_sender(Box::new(MockAlertSender::new("test")))
            .with_dedup_window(Duration::from_millis(10));

        let alert = Alert::warning("Test", "Message");
        router.route_alert(&alert).await.unwrap();

        assert_eq!(router.dedup_state.len(), 1);

        // Wait for dedup window to expire
        tokio::time::sleep(Duration::from_millis(30)).await;

        // Send again to trigger cleanup
        router.route_alert(&alert).await.unwrap();

        // Old entry should be cleaned up, new entry added
        assert_eq!(router.dedup_state.len(), 1);
    }
}
