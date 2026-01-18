//! Alert sender trait and implementations.
//!
//! This module defines the `AlertSender` trait for sending alerts to various
//! destinations, following the same pattern as `HealthChecker` and `SecretsProvider`.

use crate::models::alert::{Alert, GrafanaOnCallPayload};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::debug;

/// Errors that can occur when sending alerts.
#[derive(Debug, Error)]
pub enum AlertSenderError {
    /// Failed to send HTTP request.
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// The destination returned an error response.
    #[error("Destination returned error (status {status}): {body}")]
    DestinationError { status: u16, body: String },

    /// The alert was rejected by the destination.
    #[error("Alert rejected: {0}")]
    Rejected(String),

    /// A transient error that may succeed on retry.
    #[error("Transient error (retryable): {0}")]
    Transient(String),
}

impl AlertSenderError {
    /// Returns true if this error is transient and the operation should be retried.
    pub fn is_retryable(&self) -> bool {
        match self {
            AlertSenderError::Transient(_) => true,
            AlertSenderError::DestinationError { status, .. } => {
                // Retry on server errors and rate limiting
                *status >= 500 || *status == 429
            }
            AlertSenderError::RequestFailed(_) => true, // Network errors are retryable
            AlertSenderError::Rejected(_) => false,
        }
    }
}

/// Trait for sending alerts to a destination.
///
/// Implementations handle the specifics of formatting and delivering alerts
/// to a particular system (Grafana OnCall, PagerDuty, Slack, etc.).
///
/// # Design Principles
///
/// - Each sender is responsible for a single destination
/// - Senders should be stateless where possible
/// - The `send` method should be idempotent (same alert can be sent multiple times safely)
/// - Implementations should handle their own serialization format
#[async_trait]
pub trait AlertSender: Send + Sync {
    /// Returns the name of this sender for logging and metrics.
    fn name(&self) -> &str;

    /// Sends an alert to the destination.
    ///
    /// # Arguments
    /// * `alert` - The alert to send
    ///
    /// # Returns
    /// - `Ok(())` if the alert was delivered successfully
    /// - `Err(AlertSenderError)` if delivery failed
    async fn send(&self, alert: &Alert) -> Result<(), AlertSenderError>;

    /// Returns whether this sender is currently enabled.
    ///
    /// Disabled senders are skipped during routing.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Returns the timeout for this sender's operations.
    ///
    /// The router will enforce this timeout on the `send` operation.
    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

/// HTTP client trait for testability.
///
/// This abstraction allows injecting mock clients in tests while using
/// the real reqwest client in production.
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
        timeout: Duration,
    ) -> Result<HttpResponse, String>;
}

/// Simplified HTTP response for the alert system.
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Production HTTP client using reqwest.
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
        timeout: Duration,
    ) -> Result<HttpResponse, String> {
        let response = self
            .client
            .post(url)
            .json(body)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response body".to_string());

        Ok(HttpResponse { status, body })
    }
}

/// Alert sender for Grafana OnCall webhooks.
///
/// Formats alerts according to the Grafana OnCall webhook specification
/// and sends them via HTTP POST.
///
/// See: https://grafana.com/docs/oncall/latest/integrations/webhook/
pub struct GrafanaOnCallSender {
    name: String,
    webhook_url: String,
    enabled: bool,
    timeout: Duration,
    http_client: Arc<dyn HttpClient>,
}

impl GrafanaOnCallSender {
    /// Creates a new Grafana OnCall sender.
    pub fn new(name: impl Into<String>, webhook_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            webhook_url: webhook_url.into(),
            enabled: true,
            timeout: Duration::from_secs(10),
            http_client: Arc::new(ReqwestClient::new()),
        }
    }

    /// Creates a sender with a custom HTTP client (for testing).
    pub fn with_http_client(mut self, client: Arc<dyn HttpClient>) -> Self {
        self.http_client = client;
        self
    }

    /// Sets the timeout for HTTP requests.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Disables this sender.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[async_trait]
impl AlertSender for GrafanaOnCallSender {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }

    async fn send(&self, alert: &Alert) -> Result<(), AlertSenderError> {
        let payload: GrafanaOnCallPayload = alert.into();
        let body = serde_json::to_value(&payload).map_err(|e| {
            AlertSenderError::RequestFailed(format!("Failed to serialize alert: {}", e))
        })?;

        debug!(
            sender = %self.name,
            alert_id = %alert.id,
            url = %self.webhook_url,
            "Sending alert to Grafana OnCall"
        );

        let response = self
            .http_client
            .post_json(&self.webhook_url, &body, self.timeout)
            .await
            .map_err(|e| {
                // Network errors are transient
                AlertSenderError::Transient(e)
            })?;

        if response.status >= 200 && response.status < 300 {
            Ok(())
        } else if response.status == 429 {
            Err(AlertSenderError::Transient(format!(
                "Rate limited: {}",
                response.body
            )))
        } else if response.status >= 500 {
            Err(AlertSenderError::Transient(format!(
                "Server error: {}",
                response.body
            )))
        } else {
            Err(AlertSenderError::DestinationError {
                status: response.status,
                body: response.body,
            })
        }
    }
}

/// Mock alert sender for testing.
#[cfg(test)]
pub struct MockAlertSender {
    name: String,
    enabled: bool,
    should_fail: bool,
    fail_transiently: bool,
    sent_alerts: std::sync::Mutex<Vec<Alert>>,
}

#[cfg(test)]
impl MockAlertSender {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enabled: true,
            should_fail: false,
            fail_transiently: false,
            sent_alerts: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn failing(mut self) -> Self {
        self.should_fail = true;
        self
    }

    pub fn failing_transiently(mut self) -> Self {
        self.should_fail = true;
        self.fail_transiently = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn sent_alerts(&self) -> Vec<Alert> {
        self.sent_alerts.lock().unwrap().clone()
    }
}

#[cfg(test)]
#[async_trait]
impl AlertSender for MockAlertSender {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn send(&self, alert: &Alert) -> Result<(), AlertSenderError> {
        if self.should_fail {
            if self.fail_transiently {
                return Err(AlertSenderError::Transient("Mock transient failure".into()));
            }
            return Err(AlertSenderError::Rejected("Mock failure".into()));
        }

        self.sent_alerts.lock().unwrap().push(alert.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alert::Alert;

    #[test]
    fn test_sender_error_retryable() {
        assert!(AlertSenderError::Transient("timeout".into()).is_retryable());
        assert!(AlertSenderError::RequestFailed("network".into()).is_retryable());
        assert!(
            AlertSenderError::DestinationError {
                status: 500,
                body: "error".into()
            }
            .is_retryable()
        );
        assert!(
            AlertSenderError::DestinationError {
                status: 429,
                body: "rate limit".into()
            }
            .is_retryable()
        );
        assert!(
            !AlertSenderError::DestinationError {
                status: 400,
                body: "bad request".into()
            }
            .is_retryable()
        );
        assert!(!AlertSenderError::Rejected("no".into()).is_retryable());
    }

    #[tokio::test]
    async fn test_mock_sender_success() {
        let sender = MockAlertSender::new("test");
        let alert = Alert::warning("Test", "Message");

        let result = sender.send(&alert).await;
        assert!(result.is_ok());
        assert_eq!(sender.sent_alerts().len(), 1);
    }

    #[tokio::test]
    async fn test_mock_sender_failure() {
        let sender = MockAlertSender::new("test").failing();
        let alert = Alert::warning("Test", "Message");

        let result = sender.send(&alert).await;
        assert!(result.is_err());
        assert!(sender.sent_alerts().is_empty());
    }

    #[tokio::test]
    async fn test_mock_sender_disabled() {
        let sender = MockAlertSender::new("test").disabled();
        assert!(!sender.is_enabled());
    }

    struct MockHttpClient {
        response_status: u16,
        response_body: String,
    }

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn post_json(
            &self,
            _url: &str,
            _body: &serde_json::Value,
            _timeout: Duration,
        ) -> Result<HttpResponse, String> {
            Ok(HttpResponse {
                status: self.response_status,
                body: self.response_body.clone(),
            })
        }
    }

    #[tokio::test]
    async fn test_grafana_sender_success() {
        let mock_client = Arc::new(MockHttpClient {
            response_status: 200,
            response_body: "ok".to_string(),
        });

        let sender = GrafanaOnCallSender::new("test", "https://example.com/webhook")
            .with_http_client(mock_client);

        let alert = Alert::critical("DB Down", "Database connection lost");
        let result = sender.send(&alert).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_grafana_sender_rate_limited() {
        let mock_client = Arc::new(MockHttpClient {
            response_status: 429,
            response_body: "too many requests".to_string(),
        });

        let sender = GrafanaOnCallSender::new("test", "https://example.com/webhook")
            .with_http_client(mock_client);

        let alert = Alert::warning("Test", "Message");
        let result = sender.send(&alert).await;

        assert!(matches!(result, Err(AlertSenderError::Transient(_))));
    }

    #[tokio::test]
    async fn test_grafana_sender_server_error() {
        let mock_client = Arc::new(MockHttpClient {
            response_status: 503,
            response_body: "service unavailable".to_string(),
        });

        let sender = GrafanaOnCallSender::new("test", "https://example.com/webhook")
            .with_http_client(mock_client);

        let alert = Alert::warning("Test", "Message");
        let result = sender.send(&alert).await;

        assert!(matches!(result, Err(AlertSenderError::Transient(_))));
    }

    #[tokio::test]
    async fn test_grafana_sender_client_error() {
        let mock_client = Arc::new(MockHttpClient {
            response_status: 400,
            response_body: "bad request".to_string(),
        });

        let sender = GrafanaOnCallSender::new("test", "https://example.com/webhook")
            .with_http_client(mock_client);

        let alert = Alert::warning("Test", "Message");
        let result = sender.send(&alert).await;

        assert!(matches!(
            result,
            Err(AlertSenderError::DestinationError { status: 400, .. })
        ));
    }
}
