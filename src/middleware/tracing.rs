//! Request tracing middleware for distributed tracing.

use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error;
use std::future::{ready, Ready};
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::Instrument;

/// Middleware that creates tracing spans for HTTP requests.
///
/// This middleware:
/// - Creates a span for each incoming request
/// - Propagates trace context from HTTP headers
/// - Adds request metadata to the span
/// - Logs trace IDs for correlation
pub struct RequestTracing;

impl<S, B> Transform<S, ServiceRequest> for RequestTracing
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RequestTracingMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestTracingMiddleware { service }))
    }
}

pub struct RequestTracingMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for RequestTracingMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let method = req.method().to_string();
        let path = req.path().to_string();
        let version = format!("{:?}", req.version());

        // Create a span for this request
        let span = tracing::info_span!(
            "http_request",
            method = %method,
            path = %path,
            version = %version,
            trace_id = tracing::field::Empty,
        );

        // Extract trace ID after span is created and record it
        let trace_id = crate::telemetry::trace_context::get_trace_id();
        if let Some(tid) = &trace_id {
            span.record("trace_id", tid.as_str());
        }

        let fut = self.service.call(req);

        Box::pin(
            async move {
                // Log the request with trace ID
                if let Some(tid) = trace_id {
                    tracing::info!(
                        trace_id = %tid,
                        method = %method,
                        path = %path,
                        "Processing request"
                    );
                }

                let res = fut.await?;

                // Log the response
                tracing::info!(
                    status = %res.status().as_u16(),
                    "Request completed"
                );

                Ok(res)
            }
            .instrument(span),
        )
    }
}
