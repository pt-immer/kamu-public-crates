//! Tests for the SNAP BI recipe helpers (`hash`, `timestamp`,
//! `string_to_sign`, `headers`, one-shot `sign_service` / `verify_service`).

#![cfg(feature = "snap-bi")]

use http::Method;
use kamu_snap_crypto::HmacSigner;
use kamu_snap_crypto::snap_bi::{
    AccessToken, InputError, OAuthHeaders, OAuthStringToSign, Precision, ServiceRequest, ServiceStringToSign,
    Unsigned, format_jakarta, sha256_lower_hex, sha512_lower_hex, sign_service, verify_service,
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
    let token = AccessToken::from_credential("eyJxxx").unwrap();
    let parts = ServiceStringToSign::new(
        &method,
        "/snap/v1.0/balance-inquiry",
        token,
        b"{}",
        "2024-01-01T00:00:00+07:00",
    )
    .unwrap();
    let s = parts.build();
    let body_hash = sha256_lower_hex(b"{}");
    let expected = format!("POST:/snap/v1.0/balance-inquiry:eyJxxx:{body_hash}:2024-01-01T00:00:00+07:00");
    assert_eq!(s, expected);
}

#[test]
fn oauth_string_to_sign_format() {
    let parts = OAuthStringToSign::new("client-key-123", "2024-01-01T00:00:00.000+07:00").unwrap();
    assert_eq!(parts.build(), "client-key-123|2024-01-01T00:00:00.000+07:00");
}

#[test]
fn sign_then_verify_service_round_trip() {
    let secret = b"client-secret-456";
    let method = Method::POST;
    let token = AccessToken::from_credential("bearer-token").unwrap();
    let parts = ServiceStringToSign::new(
        &method,
        "/snap/v1.0/transfer-intrabank/payment",
        token,
        br#"{"partnerReferenceNo":"123"}"#,
        "2024-05-27T10:00:00+07:00",
    )
    .unwrap();
    let sig = sign_service(secret, &parts);
    verify_service(secret, &parts, &sig).unwrap();
}

#[test]
fn verify_service_rejects_tampered_body() {
    let secret = b"secret";
    let method = Method::POST;
    let token = AccessToken::from_credential("t").unwrap();
    let parts_real =
        ServiceStringToSign::new(&method, "/p", token, b"original", "2024-01-01T00:00:00+07:00").unwrap();
    let parts_tampered =
        ServiceStringToSign::new(&method, "/p", token, b"tampered", "2024-01-01T00:00:00+07:00").unwrap();
    let sig = sign_service(secret, &parts_real);
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
fn format_jakarta_converts_instead_of_relabeling() {
    let dt = chrono::DateTime::parse_from_rfc3339("2024-05-27T03:30:45Z").unwrap();
    assert_eq!(format_jakarta(dt, Precision::Seconds), "2024-05-27T10:30:45+07:00",);
}

#[test]
fn signed_request_emits_complete_headers() {
    let method = Method::POST;
    let token = AccessToken::from_credential("credential-value").unwrap();
    let canonical = ServiceStringToSign::new(
        &method,
        "/snap/v1.0/balance-inquiry",
        token,
        b"{}",
        "2024-01-01T00:00:00+07:00",
    )
    .unwrap();
    let request: ServiceRequest<'_, Unsigned> =
        ServiceRequest::new(canonical, "partner-1", "12345", "123456789").unwrap();
    let request_debug = format!("{request:?}");
    assert!(!request_debug.contains("partner-1"));
    assert!(!request_debug.contains("credential-value"));
    let signed = request.sign(&HmacSigner::new("secret"));
    let headers = signed.headers();
    let debug = format!("{headers:?}");
    assert!(!debug.contains("partner-1"));
    assert!(!debug.contains("credential-value"));
    assert!(!debug.contains(&signed.signature().to_base64()));
    let pairs = headers.into_pairs();
    assert!(pairs.iter().any(|(k, v)| *k == "X-PARTNER-ID" && v == "partner-1"));
    assert!(pairs.iter().any(|(k, v)| *k == "Authorization" && v == "Bearer credential-value"));
}

#[test]
fn oauth_header_debug_redacts_values() {
    let headers =
        OAuthHeaders::new("client-key-secret", "2024-01-01T00:00:00.000+07:00", "signature-secret").unwrap();
    let debug = format!("{headers:?}");
    assert!(!debug.contains("client-key-secret"));
    assert!(!debug.contains("signature-secret"));
}

#[test]
fn outbound_request_rejects_invalid_external_id_and_header_bytes() {
    let method = Method::POST;
    let token = AccessToken::from_credential("token").unwrap();
    let canonical =
        ServiceStringToSign::new(&method, "/p", token, b"{}", "2024-01-01T00:00:00+07:00").unwrap();
    assert!(matches!(
        ServiceRequest::new(canonical.clone(), "partner", "12345", "123"),
        Err(InputError::InvalidExternalId),
    ));
    assert!(matches!(
        ServiceRequest::new(canonical, "partner\ninjected", "12345", "123456789"),
        Err(InputError::InvalidHeaderValue { .. }),
    ));
}

#[test]
fn bri_provider_vector_pins_path_and_body_canonicalization() {
    // BRI's published field examples, combined under its documented formula:
    // https://developers.bri.co.id/en/snap-bi/apidocs-oauth-snap-bi
    let method = Method::POST;
    let token = AccessToken::from_credential("R04XSUbnm1GXNmDiXx9ysWMpFWBr").unwrap();
    let body = br#"{"hello":"world"}"#;
    assert_eq!(sha256_lower_hex(body), "93a23971a914e5eacbf0a8d25154cda309c3c1c72fbb9914d47c60f3cb681588",);
    let canonical =
        ServiceStringToSign::new(&method, "/snap/v1.0/dummy", token, body, "2021-11-02T13:14:15.678+07:00")
            .unwrap();
    assert_eq!(
        canonical.build(),
        "POST:/snap/v1.0/dummy:R04XSUbnm1GXNmDiXx9ysWMpFWBr:\
         93a23971a914e5eacbf0a8d25154cda309c3c1c72fbb9914d47c60f3cb681588:\
         2021-11-02T13:14:15.678+07:00",
    );
    assert!(matches!(
        ServiceStringToSign::new(
            &method,
            "/snap/v1.0/dummy?reference=123",
            token,
            body,
            "2021-11-02T13:14:15.678+07:00",
        ),
        Err(InputError::QueryOrFragment),
    ));
}

#[test]
fn canonical_request_debug_is_redacted() {
    let method = Method::POST;
    let token = AccessToken::from_credential("top-secret-token").unwrap();
    let body = br#"{"accountNo":"secret-account"}"#;
    let canonical =
        ServiceStringToSign::new(&method, "/p", token, body, "2024-01-01T00:00:00+07:00").unwrap();
    let debug = format!("{canonical:?}");
    assert!(!debug.contains("top-secret-token"));
    assert!(!debug.contains("secret-account"));
    assert!(debug.contains(&format!("body_len: {}", body.len())));
}
