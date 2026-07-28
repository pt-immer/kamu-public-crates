//! Header-chain extraction round-trips for [`kamu_logging::correlation`].

use std::collections::HashMap;

use kamu_logging::correlation::{
    DEFAULT_HEADER_CHAIN, MAX_CORRELATION_ID_LEN, TraceParent, TraceParentError, extract_from_headers,
    parse_traceparent_trace_id,
};

fn get(map: &HashMap<&str, &str>, name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    map.iter().find(|(k, _)| k.to_ascii_lowercase() == lower).map(|(_, v)| (*v).to_owned())
}

#[test]
fn missing_headers_yields_none() {
    let headers: HashMap<&str, &str> = HashMap::new();
    assert!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get).is_none());
}

#[test]
fn x_request_id_wins_over_x_correlation_id() {
    let mut headers = HashMap::new();
    headers.insert("X-Request-ID", "req-abc");
    headers.insert("X-Correlation-ID", "corr-xyz");
    let id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get);
    assert_eq!(id.as_deref(), Some("req-abc"));
}

#[test]
fn x_correlation_id_used_when_x_request_id_missing() {
    let mut headers = HashMap::new();
    headers.insert("X-Correlation-ID", "corr-xyz");
    let id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get);
    assert_eq!(id.as_deref(), Some("corr-xyz"));
}

#[test]
fn empty_header_value_is_skipped() {
    let mut headers = HashMap::new();
    headers.insert("X-Request-ID", "   ");
    headers.insert("X-Correlation-ID", "corr-xyz");
    let id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get);
    assert_eq!(id.as_deref(), Some("corr-xyz"));
}

#[test]
fn traceparent_falls_back_to_trace_id_segment() {
    let mut headers = HashMap::new();
    headers.insert("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");
    let id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get);
    assert_eq!(id.as_deref(), Some("4bf92f3577b34da6a3ce929d0e0e4736"));
}

#[test]
fn malformed_traceparent_yields_none() {
    let mut headers = HashMap::new();
    headers.insert("traceparent", "not-a-traceparent");
    let id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get);
    assert!(id.is_none());
}

#[test]
fn parse_traceparent_rejects_short_trace_id() {
    assert!(parse_traceparent_trace_id("00-deadbeef-spanid-01").is_none());
}

#[test]
fn parse_traceparent_rejects_non_hex() {
    assert!(parse_traceparent_trace_id("00-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-aaaaaaaaaaaaaaaa-01").is_none());
}

#[test]
fn w3c_traceparent_conformance_vectors() {
    let valid = [
        "00-12345678901234567890123456789012-1234567890123456-00",
        "00-ffffffffffffffffffffffffffffffff-ffffffffffffffff-ff",
        "cc-12345678901234567890123456789012-1234567890123456-01",
        "cc-12345678901234567890123456789012-1234567890123456-01-future",
    ];
    for value in valid {
        assert!(TraceParent::parse(value).is_ok(), "{value}");
    }

    let invalid = [
        "00-12345678901234567890123456789012-1234567890123456-01-extra",
        "ff-12345678901234567890123456789012-1234567890123456-01",
        "00-00000000000000000000000000000000-1234567890123456-01",
        "00-12345678901234567890123456789012-0000000000000000-01",
        "00-12345678901234567890123456789012-1234567890123456-0G",
        "00-12345678901234567890123456789012-1234567890123456-0A",
        "00-1234567890123456789012345678901-1234567890123456-01",
        "00-12345678901234567890123456789012-123456789012345-01",
        "cc-12345678901234567890123456789012-1234567890123456-01.future",
    ];
    for value in invalid {
        assert!(TraceParent::parse(value).is_err(), "{value}");
    }
}

#[test]
fn parsed_traceparent_reports_specific_zero_parent_error() {
    let error = TraceParent::parse("00-12345678901234567890123456789012-0000000000000000-01")
        .expect_err("zero parent must fail");
    assert_eq!(error, TraceParentError::ZeroParentId);
}

#[test]
fn correlation_header_policy_is_bounded_visible_ascii() {
    let accepted = "a".repeat(MAX_CORRELATION_ID_LEN);
    let rejected = "b".repeat(MAX_CORRELATION_ID_LEN + 1);

    let mut headers = HashMap::new();
    headers.insert("X-Request-ID", accepted.as_str());
    assert_eq!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get).as_deref(), Some(accepted.as_str()));

    headers.insert("X-Request-ID", rejected.as_str());
    assert!(extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, get).is_none());
}
