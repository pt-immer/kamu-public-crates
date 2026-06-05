//! Actix Web integration: tracing middleware with correlation enrichment.

use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder, TracingLogger};

use crate::correlation::{DEFAULT_HEADER_CHAIN, extract_from_headers};

/// A [`RootSpanBuilder`] that adds a `correlation_id` field to the root
/// span when one of [`DEFAULT_HEADER_CHAIN`] is present on the request.
///
/// All other span fields match [`DefaultRootSpanBuilder`].
pub struct EnrichedRootSpanBuilder;

impl RootSpanBuilder for EnrichedRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let correlation_id = extract_from_headers(request.headers(), DEFAULT_HEADER_CHAIN, |h, n| {
            h.get(n).and_then(|v| v.to_str().ok()).map(str::to_owned)
        });

        let span = tracing_actix_web::root_span!(request, correlation_id = tracing::field::Empty);
        if let Some(id) = correlation_id {
            span.record("correlation_id", tracing::field::display(&id));
        }
        span
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, actix_web::Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}

/// Construct a [`TracingLogger`] middleware backed by
/// [`EnrichedRootSpanBuilder`] (the default root span enriched with a
/// `correlation_id` field).
///
/// For a custom [`RootSpanBuilder`], use [`get_actix_web_logger_with`].
///
/// ```no_run
/// use actix_web::{App, HttpServer};
/// use kamu_logging::get_actix_web_logger;
///
/// # async fn run() -> std::io::Result<()> {
/// HttpServer::new(|| App::new().wrap(get_actix_web_logger()))
///     .bind("127.0.0.1:8080")?
///     .run()
///     .await
/// # }
/// ```
#[must_use]
pub fn get_actix_web_logger() -> TracingLogger<EnrichedRootSpanBuilder> {
    TracingLogger::<EnrichedRootSpanBuilder>::new()
}

/// Construct a [`TracingLogger`] middleware backed by the supplied
/// [`RootSpanBuilder`]. Use this when the enriched default does not fit
/// (e.g., to add tenant_id, user_id, or a service-specific field).
///
/// ```no_run
/// use kamu_logging::get_actix_web_logger_with;
/// use tracing_actix_web::DefaultRootSpanBuilder;
///
/// let middleware = get_actix_web_logger_with::<DefaultRootSpanBuilder>();
/// # let _ = middleware;
/// ```
#[must_use]
pub fn get_actix_web_logger_with<RSB: RootSpanBuilder>() -> TracingLogger<RSB> {
    TracingLogger::<RSB>::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_build_middleware() {
        let _enriched = get_actix_web_logger();
        let _default = get_actix_web_logger_with::<DefaultRootSpanBuilder>();
        let _explicit = get_actix_web_logger_with::<EnrichedRootSpanBuilder>();
    }

    // `EnrichedRootSpanBuilder::on_request_start` calls `tracing_actix_web::root_span!`,
    // which unwraps a `RequestId` request extension that only the `TracingLogger`
    // middleware inserts — so the enrichment path is integration-bound (needs a full
    // `App` + test service), not unit-testable in isolation. Its correlation-id
    // extraction half is covered directly by the `correlation` module tests.
}
