//! Tests for [`ResponseCode`] defensive parser + `classify` inverse parser.

use kamu_snap_response::{Error, ResponseCode, ServiceCode};

#[test]
fn parse_well_formed_unauthorized() {
    let code = ResponseCode::parse("4011100");
    assert_eq!(code.raw(), "4011100");
    assert_eq!(code.http(), Some(http::StatusCode::UNAUTHORIZED));
    assert_eq!(code.service(), ServiceCode::new(11));
    assert_eq!(code.case(), Some(0));
}

#[test]
fn parse_well_formed_success() {
    let code = ResponseCode::parse("2001100");
    assert_eq!(code.http(), Some(http::StatusCode::OK));
    assert_eq!(code.service(), ServiceCode::new(11));
    assert_eq!(code.case(), Some(0));
}

#[test]
fn parse_malformed_preserves_raw() {
    // BRI source-doc 6-digit defect — must NOT lose the raw string.
    let code = ResponseCode::parse("500000");
    assert_eq!(code.raw(), "500000");
    assert_eq!(code.http(), None);
    assert_eq!(code.service(), None);
    assert_eq!(code.case(), None);
}

#[test]
fn parse_non_numeric_preserves_raw() {
    let code = ResponseCode::parse("ABCDEFG");
    assert_eq!(code.raw(), "ABCDEFG");
    assert_eq!(code.http(), None);
}

#[test]
fn parse_empty_preserves_raw() {
    let code = ResponseCode::parse("");
    assert_eq!(code.raw(), "");
    assert_eq!(code.http(), None);
}

#[test]
fn classify_returns_known_variant() {
    let code = ResponseCode::parse("4011100");
    let classified = code.classify().expect("known variant");
    assert!(matches!(classified, Error::Unauthorized(_)));
}

#[test]
fn classify_returns_insufficient_funds() {
    let code = ResponseCode::parse("4031114");
    let classified = code.classify().expect("known variant");
    assert!(matches!(classified, Error::InsufficientFunds));
}

#[test]
fn classify_returns_timeout() {
    let code = ResponseCode::parse("5041100");
    let classified = code.classify().expect("known variant");
    assert!(matches!(classified, Error::Timeout));
}

#[test]
fn classify_unknown_pair_returns_none() {
    // HTTP 418, case 42 — definitely not in the table.
    let code = ResponseCode::parse("4181142");
    assert!(code.classify().is_none());
}

#[test]
fn classify_malformed_returns_none() {
    let code = ResponseCode::parse("nope");
    assert!(code.classify().is_none());
}

#[test]
fn service_code_construction_validates() {
    assert!(ServiceCode::new(99).is_some());
    assert!(ServiceCode::new(100).is_none());
    assert!(ServiceCode::new(255).is_none());
}

#[test]
fn service_code_display_pads_to_two_digits() {
    let sc = ServiceCode::new(5).unwrap();
    assert_eq!(format!("{sc}"), "05");
}
