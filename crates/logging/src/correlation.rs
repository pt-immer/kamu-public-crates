//! Correlation identifiers and W3C `traceparent` parsing.

/// Default headers checked, in priority order, by [`extract_from_headers`].
pub const DEFAULT_HEADER_CHAIN: &[&str] = &["x-request-id", "x-correlation-id", "traceparent"];

/// Maximum accepted byte length for an untrusted correlation header.
pub const MAX_CORRELATION_ID_LEN: usize = 128;

/// A fully validated W3C `traceparent` value.
///
/// Version `00` must contain exactly the four fields defined by W3C Trace
/// Context. Higher versions may append opaque fields after a hyphen; this type
/// validates and exposes only the common four-field prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceParent<'a> {
    version: u8,
    trace_id: &'a str,
    parent_id: &'a str,
    trace_flags: u8,
}

impl<'a> TraceParent<'a> {
    /// Parse and validate a `traceparent` header.
    ///
    /// # Errors
    ///
    /// Returns [`TraceParentError`] when any common field or version-specific
    /// delimiter rule is invalid.
    pub fn parse(value: &'a str) -> Result<Self, TraceParentError> {
        const BASE_LEN: usize = 55;

        let bytes = value.as_bytes();
        if bytes.len() < BASE_LEN {
            return Err(TraceParentError::TooShort);
        }
        if bytes[2] != b'-' || bytes[35] != b'-' || bytes[52] != b'-' {
            return Err(TraceParentError::InvalidDelimiter);
        }

        let version_hex = value.get(0..2).ok_or(TraceParentError::InvalidVersion)?;
        if !is_lower_hex(version_hex) {
            return Err(TraceParentError::InvalidVersion);
        }
        if version_hex == "ff" {
            return Err(TraceParentError::ReservedVersion);
        }
        let version = u8::from_str_radix(version_hex, 16).map_err(|_| TraceParentError::InvalidVersion)?;

        if version == 0 && bytes.len() != BASE_LEN {
            return Err(TraceParentError::VersionZeroHasExtraFields);
        }
        if version != 0 && bytes.len() > BASE_LEN && bytes[BASE_LEN] != b'-' {
            return Err(TraceParentError::InvalidFutureVersionSuffix);
        }

        let trace_id = value.get(3..35).ok_or(TraceParentError::InvalidTraceId)?;
        if !is_lower_hex(trace_id) {
            return Err(TraceParentError::InvalidTraceId);
        }
        if trace_id.bytes().all(|byte| byte == b'0') {
            return Err(TraceParentError::ZeroTraceId);
        }

        let parent_id = value.get(36..52).ok_or(TraceParentError::InvalidParentId)?;
        if !is_lower_hex(parent_id) {
            return Err(TraceParentError::InvalidParentId);
        }
        if parent_id.bytes().all(|byte| byte == b'0') {
            return Err(TraceParentError::ZeroParentId);
        }

        let flags_hex = value.get(53..55).ok_or(TraceParentError::InvalidTraceFlags)?;
        if !is_lower_hex(flags_hex) {
            return Err(TraceParentError::InvalidTraceFlags);
        }
        let trace_flags =
            u8::from_str_radix(flags_hex, 16).map_err(|_| TraceParentError::InvalidTraceFlags)?;

        Ok(Self { version, trace_id, parent_id, trace_flags })
    }

    /// Parsed version byte.
    #[must_use]
    pub const fn version(self) -> u8 {
        self.version
    }

    /// Validated 32-character lowercase hexadecimal trace identifier.
    #[must_use]
    pub const fn trace_id(self) -> &'a str {
        self.trace_id
    }

    /// Validated 16-character lowercase hexadecimal parent identifier.
    #[must_use]
    pub const fn parent_id(self) -> &'a str {
        self.parent_id
    }

    /// Parsed trace-flags byte.
    #[must_use]
    pub const fn trace_flags(self) -> u8 {
        self.trace_flags
    }

    /// Whether the W3C sampled flag is set.
    #[must_use]
    pub const fn is_sampled(self) -> bool {
        self.trace_flags & 1 == 1
    }
}

impl<'a> TryFrom<&'a str> for TraceParent<'a> {
    type Error = TraceParentError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// Why a W3C `traceparent` header was rejected.
#[non_exhaustive]
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceParentError {
    /// The common four-field prefix is shorter than 55 bytes.
    #[error("traceparent is shorter than the 55-byte base format")]
    TooShort,
    /// A common-field separator is not a hyphen at its required position.
    #[error("traceparent contains an invalid field delimiter")]
    InvalidDelimiter,
    /// The version is not two lowercase hexadecimal digits.
    #[error("traceparent version must be two lowercase hexadecimal digits")]
    InvalidVersion,
    /// Version `ff` is reserved and invalid.
    #[error("traceparent version ff is reserved")]
    ReservedVersion,
    /// Version `00` contains fields beyond its fixed 55-byte format.
    #[error("traceparent version 00 cannot contain extra fields")]
    VersionZeroHasExtraFields,
    /// A higher-version suffix does not start with a hyphen.
    #[error("a future traceparent version suffix must start with a hyphen")]
    InvalidFutureVersionSuffix,
    /// The trace ID is not 32 lowercase hexadecimal digits.
    #[error("traceparent trace ID must be 32 lowercase hexadecimal digits")]
    InvalidTraceId,
    /// The trace ID is the forbidden all-zero value.
    #[error("traceparent trace ID cannot be all zero")]
    ZeroTraceId,
    /// The parent ID is not 16 lowercase hexadecimal digits.
    #[error("traceparent parent ID must be 16 lowercase hexadecimal digits")]
    InvalidParentId,
    /// The parent ID is the forbidden all-zero value.
    #[error("traceparent parent ID cannot be all zero")]
    ZeroParentId,
    /// Trace flags are not two lowercase hexadecimal digits.
    #[error("traceparent flags must be two lowercase hexadecimal digits")]
    InvalidTraceFlags,
}

/// Extract a correlation id from a header bag using a configurable getter.
///
/// `headers` is opaque; the caller supplies `get` which performs the
/// case-insensitive lookup natural to its header type. `chain` is the
/// ordered list of header names to try. The first valid value wins.
///
/// `get` must return a **single** header value — the first occurrence when a
/// header repeats. Backends that join repeated headers (e.g. the Fetch
/// `Headers.get`, which comma-joins) would otherwise yield a comma-joined
/// correlation id, diverging from single-value backends like
/// `actix_web::http::HeaderMap::get`; take the first comma-separated segment in
/// `get` if your header type joins.
///
/// Non-`traceparent` values must contain 1–128 visible ASCII bytes. This rejects
/// control characters, whitespace, non-ASCII text, and oversized untrusted
/// fields before they enter a span. `traceparent` values are fully validated by
/// [`TraceParent`] before their trace ID is returned.
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
                if let Ok(traceparent) = TraceParent::parse(trimmed) {
                    return Some(traceparent.trace_id().to_owned());
                }
                continue;
            }
            if is_valid_correlation_id(trimmed) {
                return Some(trimmed.to_owned());
            }
        }
    }
    None
}

/// Parse the trace ID from a fully valid W3C `traceparent` header.
///
/// This compatibility helper delegates to [`TraceParent::parse`]; callers that
/// need the parent ID, flags, or rejection reason should use [`TraceParent`].
#[must_use]
pub fn parse_traceparent_trace_id(traceparent: &str) -> Option<&str> {
    TraceParent::parse(traceparent).ok().map(TraceParent::trace_id)
}

fn is_lower_hex(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_valid_correlation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CORRELATION_ID_LEN
        && value.bytes().all(|byte| byte.is_ascii_graphic())
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
        // W3C-invalid: the all-zero (null) trace-id is rejected, not echoed back.
        assert_eq!(
            parse_traceparent_trace_id("00-00000000000000000000000000000000-00f067aa0ba902b7-01"),
            None
        );
        // W3C-invalid: reserved version `ff` is rejected.
        assert_eq!(
            parse_traceparent_trace_id("ff-4bf92f3577b34da6a3ce929d0e0e4736-0123456789abcdef-01"),
            None
        );
        // Malformed version (not two hex digits) is rejected.
        assert_eq!(
            parse_traceparent_trace_id("0-4bf92f3577b34da6a3ce929d0e0e4736-0123456789abcdef-01"),
            None
        );
    }

    #[test]
    fn traceparent_exposes_all_validated_fields() {
        let parsed =
            TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-03").expect("valid");
        assert_eq!(parsed.version(), 0);
        assert_eq!(parsed.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(parsed.parent_id(), "00f067aa0ba902b7");
        assert_eq!(parsed.trace_flags(), 3);
        assert!(parsed.is_sampled());
    }

    #[test]
    fn traceparent_applies_version_specific_suffix_rules() {
        let base = "cc-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        assert!(TraceParent::parse(base).is_ok());
        assert!(TraceParent::parse(&format!("{base}-future-fields")).is_ok());
        assert!(matches!(
            TraceParent::parse("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra"),
            Err(TraceParentError::VersionZeroHasExtraFields)
        ));
        assert!(matches!(
            TraceParent::parse(&format!("{base}.not-a-field")),
            Err(TraceParentError::InvalidFutureVersionSuffix)
        ));
    }

    #[test]
    fn traceparent_rejects_every_invalid_common_field() {
        let cases = [
            ("00-4bf92f3577b34da6a3ce929d0e0e4736", TraceParentError::TooShort),
            ("ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01", TraceParentError::ReservedVersion),
            ("00-4BF92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01", TraceParentError::InvalidTraceId),
            ("00-00000000000000000000000000000000-00f067aa0ba902b7-01", TraceParentError::ZeroTraceId),
            ("00-4bf92f3577b34da6a3ce929d0e0e4736-00F067aa0ba902b7-01", TraceParentError::InvalidParentId),
            ("00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01", TraceParentError::ZeroParentId),
            ("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-0A", TraceParentError::InvalidTraceFlags),
            ("00_4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01", TraceParentError::InvalidDelimiter),
        ];

        for (value, expected) in cases {
            assert_eq!(TraceParent::parse(value), Err(expected), "{value}");
        }
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
    fn extract_rejects_untrusted_correlation_values_then_falls_back() {
        let oversized = "x".repeat(MAX_CORRELATION_ID_LEN + 1);
        for rejected in ["contains space", "line\nbreak", "café", oversized.as_str()] {
            let headers = [("x-request-id", rejected), ("x-correlation-id", "safe-id")];
            assert_eq!(
                extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, lookup),
                Some("safe-id".to_owned())
            );
        }

        let boundary = "x".repeat(MAX_CORRELATION_ID_LEN);
        let headers = [("x-request-id", boundary.as_str())];
        assert_eq!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, lookup), Some(boundary));
    }

    #[test]
    fn span_helpers_construct() {
        let _span = span("abc");
        let out = with_id("xyz", || 7);
        assert_eq!(out, 7);
    }
}
