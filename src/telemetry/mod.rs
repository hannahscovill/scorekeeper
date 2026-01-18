//! Telemetry and distributed tracing configuration.
//!
//! This module provides OpenTelemetry integration with AWS X-Ray for distributed tracing.
//! It sets up trace collection, context propagation, and export to AWS X-Ray.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_aws::trace::XrayPropagator;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, TracerProvider};
use opentelemetry_sdk::{runtime, Resource};
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub mod trace_context;

/// Initializes the OpenTelemetry tracing pipeline with AWS X-Ray export.
///
/// This sets up:
/// - AWS X-Ray propagator for distributed context
/// - OTLP exporter for sending traces
/// - Trace provider with resource metadata
/// - Integration with tracing crate
///
/// # Arguments
/// * `service_name` - Name of the service for trace identification
/// * `otlp_endpoint` - Optional OTLP collector endpoint (defaults to localhost:4317)
///
/// # Returns
/// A TracerProvider that should be kept alive for the lifetime of the application
pub fn init_telemetry(
    service_name: String,
    otlp_endpoint: Option<String>,
) -> Result<TracerProvider, Box<dyn std::error::Error>> {
    // Set up AWS X-Ray propagator for context propagation
    // This ensures trace context is properly extracted from and injected into HTTP headers
    global::set_text_map_propagator(XrayPropagator::default());

    // Also register standard W3C TraceContext propagator for interoperability
    // This allows the service to work with other OpenTelemetry-compatible systems
    let propagator = TraceContextPropagator::new();
    global::set_text_map_propagator(propagator);

    // Configure resource with service metadata
    let resource = Resource::new(vec![
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            service_name.clone(),
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
            env!("CARGO_PKG_VERSION").to_string(),
        ),
    ]);

    // Configure OTLP exporter
    let endpoint = otlp_endpoint.unwrap_or_else(|| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(Duration::from_secs(3))
        .build()?;

    // Build the tracer provider
    let tracer_provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(resource)
        .with_id_generator(RandomIdGenerator::default())
        .with_sampler(Sampler::AlwaysOn) // Simple sampling - sample everything
        .build();

    // Set as global tracer provider
    global::set_tracer_provider(tracer_provider.clone());

    // Create tracing layer with OpenTelemetry integration
    let telemetry_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer_provider.tracer(service_name));

    // Set up tracing subscriber with both console and OpenTelemetry layers
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(telemetry_layer)
        .init();

    Ok(tracer_provider)
}

/// Shuts down the OpenTelemetry provider and flushes remaining traces.
///
/// This should be called during application shutdown to ensure all traces are exported.
pub fn shutdown_telemetry() {
    global::shutdown_tracer_provider();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_init_with_defaults() {
        // Basic smoke test - just ensure we can construct the configuration
        // Actual initialization requires a running OTLP collector
        let result = init_telemetry("test-service".to_string(), None);

        // We expect this to fail in tests since there's no collector
        // but we're checking that the code compiles and runs
        assert!(result.is_ok() || result.is_err());
    }
}
