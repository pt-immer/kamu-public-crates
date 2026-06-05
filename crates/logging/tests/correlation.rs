//! Header-chain extraction round-trips for [`kamu_logging::correlation`].

use std::collections::HashMap;

use kamu_logging::correlation::{DEFAULT_HEADER_CHAIN, extract_from_headers, parse_traceparent_trace_id};

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
