//! Trace context utilities for extracting and using trace IDs.

use opentelemetry::trace::{SpanContext, TraceContextExt};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Extracts the trace ID from the current tracing span.
///
/// Returns a hex-encoded trace ID string that can be used for logging
/// and correlation with distributed traces in AWS X-Ray.
///
/// # Returns
/// Some(trace_id) if a valid trace context exists, None otherwise
pub fn get_trace_id() -> Option<String> {
    let context = Span::current().context();
    let span = context.span();
    let span_context = span.span_context();

    if span_context.is_valid() {
        Some(format!("{:032x}", span_context.trace_id()))
    } else {
        None
    }
}

/// Extracts the span ID from the current tracing span.
///
/// Returns a hex-encoded span ID string.
///
/// # Returns
/// Some(span_id) if a valid span context exists, None otherwise
pub fn get_span_id() -> Option<String> {
    let context = Span::current().context();
    let span = context.span();
    let span_context = span.span_context();

    if span_context.is_valid() {
        Some(format!("{:016x}", span_context.span_id()))
    } else {
        None
    }
}

/// Returns the current span context for manual propagation.
///
/// This can be used when you need to manually propagate trace context
/// to external systems or asynchronous tasks.
pub fn current_span_context() -> SpanContext {
    let context = Span::current().context();
    let span = context.span();
    span.span_context().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_trace_id_without_context() {
        // Without an active span, should return None
        let trace_id = get_trace_id();
        assert!(trace_id.is_none());
    }

    #[test]
    fn test_get_span_id_without_context() {
        // Without an active span, should return None
        let span_id = get_span_id();
        assert!(span_id.is_none());
    }
}
