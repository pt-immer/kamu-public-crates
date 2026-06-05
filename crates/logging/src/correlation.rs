//! Correlation-id helpers for distributed tracing.
//!
//! Provides a header-chain extractor and span constructors so consumers
//! (HTTP middlewares, queue workers) can attach a consistent
//! `correlation_id` field to every span without re-implementing the
//! convention per service.

/// Default headers checked, in priority order, by [`extract_from_headers`].
pub const DEFAULT_HEADER_CHAIN: &[&str] = &["x-request-id", "x-correlation-id", "traceparent"];

/// Extract a correlation id from a header bag using a configurable getter.
///
/// `headers` is opaque; the caller supplies `get` which performs the
/// case-insensitive lookup natural to its header type. `chain` is the
/// ordered list of header names to try. The first header that yields a
/// non-empty value wins.
///
/// `traceparent` values are reduced to their trace-id segment per W3C
/// Trace Context (`<version>-<trace-id>-<span-id>-<flags>`).
pub fn extract_from_headers<H, F>(headers: &H, chain: &[&str], get: F) -> Option<String>
where
    F: Fn(&H, &str) -> Option<String>,
{
    for &name in chain {
        if let Some(value) = get(headers, name) {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }
            if name.eq_ignore_ascii_case("traceparent") {
                if let Some(trace_id) = parse_traceparent_trace_id(trimmed) {
                    return Some(trace_id.to_owned());
                }
                continue;
            }
            return Some(trimmed.to_owned());
        }
    }
    None
}

/// Parse the trace-id segment from a W3C `traceparent` header.
///
/// Format: `<version>-<trace-id>-<parent-id>-<trace-flags>`. Returns the
/// trace-id segment if present and well-formed-enough to use as an id.
#[must_use]
pub fn parse_traceparent_trace_id(traceparent: &str) -> Option<&str> {
    let mut parts = traceparent.split('-');
    let _version = parts.next()?;
    let trace_id = parts.next()?;
    if trace_id.len() != 32 || !trace_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(trace_id)
}

/// Run a synchronous closure inside a span carrying `correlation_id`.
///
/// ```
/// # use kamu_logging::correlation::with_id;
/// with_id("req-abc123", || {
///     tracing::info!("inside correlation span");
/// });
/// ```
pub fn with_id<F, R>(id: impl Into<String>, f: F) -> R
where
    F: FnOnce() -> R,
{
    let id = id.into();
    let span = tracing::info_span!("correlation", correlation_id = %id);
    let _enter = span.enter();
    f()
}

/// Build a span carrying `correlation_id`. Useful when the caller wants to
/// control entry/exit explicitly or attach to a future via
/// [`tracing::Instrument`].
#[must_use]
pub fn span(id: impl AsRef<str>) -> tracing::Span {
    tracing::info_span!("correlation", correlation_id = %id.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<const N: usize>(headers: &[(&str, &str); N], name: &str) -> Option<String> {
        headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| (*v).to_owned())
    }

    #[test]
    fn parse_traceparent_accepts_well_formed_and_rejects_malformed() {
        let valid = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        assert_eq!(parse_traceparent_trace_id(valid), Some("4bf92f3577b34da6a3ce929d0e0e4736"));
        assert_eq!(parse_traceparent_trace_id(""), None);
        assert_eq!(parse_traceparent_trace_id("00"), None);
        assert_eq!(parse_traceparent_trace_id("00-deadbeef-x-01"), None); // wrong length
        assert_eq!(
            parse_traceparent_trace_id("00-zzzz2f3577b34da6a3ce929d0e0e4736-x-01"),
            None // right length, non-hex
        );
    }

    #[test]
    fn extract_skips_empty_then_takes_first_non_empty_in_chain() {
        let headers = [("x-request-id", "   "), ("x-correlation-id", "corr-9")];
        assert_eq!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, lookup), Some("corr-9".to_owned()));
    }

    #[test]
    fn extract_reduces_traceparent_to_trace_id() {
        let headers = [("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")];
        assert_eq!(
            extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, lookup),
            Some("4bf92f3577b34da6a3ce929d0e0e4736".to_owned())
        );
    }

    #[test]
    fn extract_skips_malformed_traceparent() {
        let headers = [("traceparent", "garbage")];
        assert_eq!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, lookup), None);
    }

    #[test]
    fn extract_returns_none_when_chain_absent() {
        let headers: [(&str, &str); 0] = [];
        assert_eq!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, lookup), None);
    }

    #[test]
    fn span_helpers_construct() {
        let _span = span("abc");
        let out = with_id("xyz", || 7);
        assert_eq!(out, 7);
    }
}
