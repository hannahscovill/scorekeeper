//! Health check service providing production-ready health monitoring.
//!
//! This module implements a composable health check system that follows
//! Kubernetes health probe patterns:
//!
//! - **Liveness probes**: Is the process running and not deadlocked?
//! - **Readiness probes**: Can the service handle traffic (dependencies available)?
//!
//! # Architecture
//!
//! The health check system is built around the `HealthChecker` trait, which
//! allows for easy extension with new dependency checks. Each checker is
//! responsible for a single concern (database connectivity, external service
//! availability, etc.).
//!
//! The `HealthService` aggregates multiple checkers and provides both
//! individual and composite health status.
//!
//! # Example
//!
//! ```ignore
//! let health_service = HealthService::new()
//!     .with_checker(Box::new(DatabaseHealthChecker::new(db.clone())))
//!     .with_checker(Box::new(ExternalApiChecker::new(client)));
//!
//! let status = health_service.check_readiness().await;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Health status for a single component or the overall system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Component is healthy and operating normally.
    Healthy,
    /// Component is degraded but still functional.
    Degraded,
    /// Component is unhealthy and cannot function.
    Unhealthy,
}

impl HealthStatus {
    /// Returns true if the status indicates the component can handle requests.
    pub fn is_ok(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }

    /// Combines two health statuses, returning the worse of the two.
    pub fn combine(self, other: HealthStatus) -> HealthStatus {
        match (self, other) {
            (HealthStatus::Unhealthy, _) | (_, HealthStatus::Unhealthy) => HealthStatus::Unhealthy,
            (HealthStatus::Degraded, _) | (_, HealthStatus::Degraded) => HealthStatus::Degraded,
            _ => HealthStatus::Healthy,
        }
    }
}

/// Result of a single health check operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Name of the component being checked.
    pub name: String,
    /// Current health status.
    pub status: HealthStatus,
    /// Human-readable description of the current state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Time taken to perform the check in milliseconds.
    pub latency_ms: u64,
    /// Timestamp when this check was performed (Unix epoch milliseconds).
    pub checked_at: u64,
}

impl HealthCheckResult {
    /// Creates a new healthy result.
    pub fn healthy(name: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Healthy,
            message: None,
            latency_ms,
            checked_at: current_timestamp_ms(),
        }
    }

    /// Creates a new degraded result with a message.
    pub fn degraded(name: impl Into<String>, message: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Degraded,
            message: Some(message.into()),
            latency_ms,
            checked_at: current_timestamp_ms(),
        }
    }

    /// Creates a new unhealthy result with a message.
    pub fn unhealthy(name: impl Into<String>, message: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Unhealthy,
            message: Some(message.into()),
            latency_ms,
            checked_at: current_timestamp_ms(),
        }
    }
}

/// Aggregate health response containing all component checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Overall system health status.
    pub status: HealthStatus,
    /// Individual component check results.
    pub checks: HashMap<String, HealthCheckResult>,
    /// Service version for debugging.
    pub version: String,
    /// Total time to perform all checks in milliseconds.
    pub total_latency_ms: u64,
}

/// Simple liveness response for Kubernetes liveness probes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessResponse {
    pub status: HealthStatus,
    pub uptime_seconds: u64,
}

/// Trait for implementing health checks.
///
/// Each health checker is responsible for checking a single component
/// or dependency. Implementations should be fast and non-blocking where
/// possible.
///
/// # Timeout Handling
///
/// The `HealthService` applies timeouts to all health checks. If a check
/// exceeds its timeout, it will be marked as unhealthy with a timeout message.
/// Implementations do not need to handle timeouts themselves.
#[async_trait::async_trait]
pub trait HealthChecker: Send + Sync {
    /// Returns the name of the component being checked.
    fn name(&self) -> &str;

    /// Performs the health check and returns the result.
    ///
    /// This method should check the health of a single component and return
    /// quickly. Long-running checks should be designed to fail fast.
    async fn check(&self) -> HealthCheckResult;

    /// Returns whether this check is required for readiness.
    ///
    /// If true, a failing check will cause the readiness probe to fail.
    /// If false, a failing check will only cause a degraded status.
    fn is_critical(&self) -> bool {
        true
    }
}

/// Health checker for the in-memory database.
///
/// This checker verifies that the database lock can be acquired,
/// which indicates the database is not deadlocked.
pub struct DatabaseHealthChecker {
    db: Arc<crate::db::InMemoryDb>,
}

impl DatabaseHealthChecker {
    pub fn new(db: Arc<crate::db::InMemoryDb>) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl HealthChecker for DatabaseHealthChecker {
    fn name(&self) -> &str {
        "database"
    }

    async fn check(&self) -> HealthCheckResult {
        let start = Instant::now();

        // Try to acquire a read lock on the database
        // This verifies the RwLock is not poisoned or deadlocked
        match self.db.get_all_scores() {
            Ok(_) => {
                let latency = start.elapsed().as_millis() as u64;
                HealthCheckResult::healthy("database", latency)
            }
            Err(e) => {
                let latency = start.elapsed().as_millis() as u64;
                HealthCheckResult::unhealthy("database", format!("Database error: {}", e), latency)
            }
        }
    }
}

/// Health service that aggregates multiple health checkers.
///
/// This service manages a collection of health checkers and provides
/// methods for liveness and readiness probes.
pub struct HealthService {
    checkers: Vec<Box<dyn HealthChecker>>,
    start_time: Instant,
    version: String,
    check_timeout: Duration,
    /// Cached readiness result with expiration
    cache: Arc<RwLock<Option<CachedHealthResult>>>,
    cache_ttl: Duration,
}

struct CachedHealthResult {
    response: HealthResponse,
    cached_at: Instant,
}

impl HealthService {
    /// Creates a new health service.
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            checkers: Vec::new(),
            start_time: Instant::now(),
            version: version.into(),
            check_timeout: Duration::from_secs(5),
            cache: Arc::new(RwLock::new(None)),
            cache_ttl: Duration::from_secs(1),
        }
    }

    /// Adds a health checker to the service.
    pub fn with_checker(mut self, checker: Box<dyn HealthChecker>) -> Self {
        self.checkers.push(checker);
        self
    }

    /// Sets the timeout for individual health checks.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.check_timeout = timeout;
        self
    }

    /// Sets the cache TTL for readiness checks.
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Performs a liveness check.
    ///
    /// Liveness checks are minimal - they only verify that the process
    /// is running and can respond. This should always succeed unless
    /// the process is deadlocked.
    pub fn check_liveness(&self) -> LivenessResponse {
        LivenessResponse {
            status: HealthStatus::Healthy,
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }

    /// Performs a readiness check with caching.
    ///
    /// Readiness checks verify that all dependencies are available and
    /// the service can handle traffic. Results are cached briefly to
    /// prevent thundering herd on health endpoints.
    pub async fn check_readiness(&self) -> HealthResponse {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.cached_at.elapsed() < self.cache_ttl {
                    return cached.response.clone();
                }
            }
        }

        // Perform fresh check
        let response = self.perform_readiness_check().await;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedHealthResult {
                response: response.clone(),
                cached_at: Instant::now(),
            });
        }

        response
    }

    /// Performs the actual readiness check without caching.
    async fn perform_readiness_check(&self) -> HealthResponse {
        let start = Instant::now();
        let mut checks = HashMap::new();
        let mut overall_status = HealthStatus::Healthy;

        for checker in &self.checkers {
            let result = tokio::time::timeout(self.check_timeout, checker.check()).await;

            let check_result = match result {
                Ok(r) => r,
                Err(_) => HealthCheckResult::unhealthy(
                    checker.name(),
                    "Health check timed out",
                    self.check_timeout.as_millis() as u64,
                ),
            };

            // Update overall status based on check result and criticality
            if checker.is_critical() {
                overall_status = overall_status.combine(check_result.status);
            } else if check_result.status == HealthStatus::Unhealthy {
                overall_status = overall_status.combine(HealthStatus::Degraded);
            }

            checks.insert(check_result.name.clone(), check_result);
        }

        HealthResponse {
            status: overall_status,
            checks,
            version: self.version.clone(),
            total_latency_ms: start.elapsed().as_millis() as u64,
        }
    }
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

/// Returns the current timestamp in milliseconds since Unix epoch.
fn current_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_is_ok() {
        assert!(HealthStatus::Healthy.is_ok());
        assert!(HealthStatus::Degraded.is_ok());
        assert!(!HealthStatus::Unhealthy.is_ok());
    }

    #[test]
    fn test_health_status_combine() {
        assert_eq!(
            HealthStatus::Healthy.combine(HealthStatus::Healthy),
            HealthStatus::Healthy
        );
        assert_eq!(
            HealthStatus::Healthy.combine(HealthStatus::Degraded),
            HealthStatus::Degraded
        );
        assert_eq!(
            HealthStatus::Healthy.combine(HealthStatus::Unhealthy),
            HealthStatus::Unhealthy
        );
        assert_eq!(
            HealthStatus::Degraded.combine(HealthStatus::Unhealthy),
            HealthStatus::Unhealthy
        );
    }

    #[test]
    fn test_health_check_result_constructors() {
        let healthy = HealthCheckResult::healthy("test", 10);
        assert_eq!(healthy.status, HealthStatus::Healthy);
        assert!(healthy.message.is_none());

        let degraded = HealthCheckResult::degraded("test", "slow response", 100);
        assert_eq!(degraded.status, HealthStatus::Degraded);
        assert_eq!(degraded.message, Some("slow response".to_string()));

        let unhealthy = HealthCheckResult::unhealthy("test", "connection failed", 5000);
        assert_eq!(unhealthy.status, HealthStatus::Unhealthy);
        assert_eq!(unhealthy.message, Some("connection failed".to_string()));
    }

    #[test]
    fn test_liveness_check() {
        let service = HealthService::new("1.0.0");
        let response = service.check_liveness();
        assert_eq!(response.status, HealthStatus::Healthy);
    }

    struct MockHealthyChecker;

    #[async_trait::async_trait]
    impl HealthChecker for MockHealthyChecker {
        fn name(&self) -> &str {
            "mock_healthy"
        }

        async fn check(&self) -> HealthCheckResult {
            HealthCheckResult::healthy("mock_healthy", 1)
        }
    }

    struct MockUnhealthyChecker;

    #[async_trait::async_trait]
    impl HealthChecker for MockUnhealthyChecker {
        fn name(&self) -> &str {
            "mock_unhealthy"
        }

        async fn check(&self) -> HealthCheckResult {
            HealthCheckResult::unhealthy("mock_unhealthy", "always fails", 1)
        }
    }

    struct MockNonCriticalChecker;

    #[async_trait::async_trait]
    impl HealthChecker for MockNonCriticalChecker {
        fn name(&self) -> &str {
            "mock_non_critical"
        }

        async fn check(&self) -> HealthCheckResult {
            HealthCheckResult::unhealthy("mock_non_critical", "non-critical failure", 1)
        }

        fn is_critical(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn test_readiness_all_healthy() {
        let service = HealthService::new("1.0.0")
            .with_checker(Box::new(MockHealthyChecker))
            .with_cache_ttl(Duration::from_millis(0)); // Disable cache for testing

        let response = service.check_readiness().await;
        assert_eq!(response.status, HealthStatus::Healthy);
        assert_eq!(response.checks.len(), 1);
    }

    #[tokio::test]
    async fn test_readiness_critical_unhealthy() {
        let service = HealthService::new("1.0.0")
            .with_checker(Box::new(MockUnhealthyChecker))
            .with_cache_ttl(Duration::from_millis(0));

        let response = service.check_readiness().await;
        assert_eq!(response.status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_readiness_non_critical_unhealthy_degrades() {
        let service = HealthService::new("1.0.0")
            .with_checker(Box::new(MockHealthyChecker))
            .with_checker(Box::new(MockNonCriticalChecker))
            .with_cache_ttl(Duration::from_millis(0));

        let response = service.check_readiness().await;
        assert_eq!(response.status, HealthStatus::Degraded);
        assert_eq!(response.checks.len(), 2);
    }

    struct MockSlowChecker;

    #[async_trait::async_trait]
    impl HealthChecker for MockSlowChecker {
        fn name(&self) -> &str {
            "mock_slow"
        }

        async fn check(&self) -> HealthCheckResult {
            tokio::time::sleep(Duration::from_secs(10)).await;
            HealthCheckResult::healthy("mock_slow", 10000)
        }
    }

    #[tokio::test]
    async fn test_readiness_timeout() {
        let service = HealthService::new("1.0.0")
            .with_checker(Box::new(MockSlowChecker))
            .with_timeout(Duration::from_millis(100))
            .with_cache_ttl(Duration::from_millis(0));

        let response = service.check_readiness().await;
        assert_eq!(response.status, HealthStatus::Unhealthy);
        let check = response.checks.get("mock_slow").unwrap();
        assert!(check.message.as_ref().unwrap().contains("timed out"));
    }
}
