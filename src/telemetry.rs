//! OpenTelemetry initialization for the scorekeeper service.

use crate::config::Environment;
use opentelemetry::trace::TracerProvider;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Initialize telemetry (tracing + OpenTelemetry).
/// Call this at the start of main(), before any other initialization.
pub fn init_telemetry(environment: &Environment) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,scorekeeper=debug"));

    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    // Build OpenTelemetry exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&otel_endpoint)
        .build()
        .expect("Failed to create OTLP exporter");

    // Build tracer provider
    let tracer_provider = sdktrace::TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(Resource::new(vec![
            KeyValue::new("service.name", "scorekeeper"),
            KeyValue::new(
                "service.version",
                std::env::var("COMMIT_HASH").unwrap_or_else(|_| "dev-0000000".into()),
            ),
            KeyValue::new("deployment.environment", environment.as_str()),
        ]))
        .build();

    let tracer = tracer_provider.tracer("scorekeeper");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // Format layer (JSON for production, pretty for local dev)
    let fmt_layer = if environment.is_production() {
        tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .boxed()
    } else {
        tracing_subscriber::fmt::layer()
            .pretty()
            .with_target(true)
            .boxed()
    };

    // Compose and init subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(otel_layer)
        .init();

    // Store provider globally for shutdown
    opentelemetry::global::set_tracer_provider(tracer_provider);
}

/// Shutdown OpenTelemetry gracefully (flush pending spans).
pub fn shutdown_telemetry() {
    opentelemetry::global::shutdown_tracer_provider();
}
