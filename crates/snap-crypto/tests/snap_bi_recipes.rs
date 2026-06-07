//! Tests for the SNAP BI recipe helpers (`hash`, `timestamp`,
//! `string_to_sign`, `headers`, one-shot `sign_service` / `verify_service`).

#![cfg(feature = "snap-bi")]

use http::Method;
use kamu_snap_crypto::snap_bi::headers::ServiceHeaders;
use kamu_snap_crypto::snap_bi::{
    OAuthStringToSign, Precision, ServiceStringToSign, format_jakarta, sha256_lower_hex, sha512_lower_hex,
    sign_service, verify_service,
};

#[test]
fn sha256_lower_hex_empty_string() {
    // NIST: SHA-256 of empty input
    assert_eq!(sha256_lower_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}

#[test]
fn sha256_lower_hex_abc() {
    assert_eq!(sha256_lower_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn sha512_lower_hex_empty_string() {
    assert_eq!(
        sha512_lower_hex(b""),
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
    );
}

#[test]
fn service_string_to_sign_format() {
    let method = Method::POST;
    let parts = ServiceStringToSign {
        method: &method,
        path: "/snap/v1.0/balance-inquiry",
        access_token: "eyJxxx",
        body: b"{}",
        timestamp: "2024-01-01T00:00:00+07:00",
    };
    let s = parts.build();
    let body_hash = sha256_lower_hex(b"{}");
    let expected = format!("POST:/snap/v1.0/balance-inquiry:eyJxxx:{body_hash}:2024-01-01T00:00:00+07:00");
    assert_eq!(s, expected);
}

#[test]
fn oauth_string_to_sign_format() {
    let parts = OAuthStringToSign { client_id: "client-key-123", timestamp: "2024-01-01T00:00:00.000+07:00" };
    assert_eq!(parts.build(), "client-key-123|2024-01-01T00:00:00.000+07:00");
}

#[test]
fn sign_then_verify_service_round_trip() {
    let secret = b"client-secret-456";
    let method = Method::POST;
    let parts = ServiceStringToSign {
        method: &method,
        path: "/snap/v1.0/transfer-intrabank/payment",
        access_token: "bearer-token",
        body: br#"{"partnerReferenceNo":"123"}"#,
        timestamp: "2024-05-27T10:00:00+07:00",
    };
    let sig = sign_service(secret, &parts).unwrap();
    verify_service(secret, &parts, &sig).unwrap();
}

#[test]
fn verify_service_rejects_tampered_body() {
    let secret = b"secret";
    let method = Method::POST;
    let parts_real = ServiceStringToSign {
        method: &method,
        path: "/p",
        access_token: "t",
        body: b"original",
        timestamp: "2024-01-01T00:00:00+07:00",
    };
    let parts_tampered = ServiceStringToSign {
        method: &method,
        path: "/p",
        access_token: "t",
        body: b"tampered",
        timestamp: "2024-01-01T00:00:00+07:00",
    };
    let sig = sign_service(secret, &parts_real).unwrap();
    assert!(verify_service(secret, &parts_tampered, &sig).is_err());
}

#[test]
fn timestamp_format_seconds() {
    let dt = chrono::DateTime::parse_from_rfc3339("2024-05-27T10:30:45+07:00").unwrap();
    assert_eq!(format_jakarta(dt, Precision::Seconds), "2024-05-27T10:30:45+07:00");
}

#[test]
fn timestamp_format_millis() {
    let dt = chrono::DateTime::parse_from_rfc3339("2024-05-27T10:30:45.123+07:00").unwrap();
    assert_eq!(format_jakarta(dt, Precision::Millis), "2024-05-27T10:30:45.123+07:00");
}

#[test]
fn service_headers_builder_rejects_missing_fields() {
    let result = ServiceHeaders::builder().partner_id("p").channel_id("c").build();
    assert!(result.is_err(), "builder should reject incomplete state");
}

#[test]
fn service_headers_builder_emits_pairs() {
    let h = ServiceHeaders::builder()
        .partner_id("partner-1")
        .channel_id("chan")
        .external_id("123456789")
        .timestamp("2024-01-01T00:00:00+07:00")
        .signature("sig")
        .bearer_token("token")
        .build()
        .unwrap();
    let pairs = h.into_pairs();
    assert!(pairs.iter().any(|(k, v)| *k == "X-PARTNER-ID" && v == "partner-1"));
    assert!(pairs.iter().any(|(k, v)| *k == "Authorization" && v == "Bearer token"));
}
