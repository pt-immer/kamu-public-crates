//! Inbound SNAP BI request validation and verification.

#![cfg(feature = "snap-bi")]

use http::Method;
use kamu_snap_crypto::snap_bi::{
    AccessToken, AuthorizationError, CanonicalPath, InputError, ServiceRequestParts, ServiceStringToSign,
    ServiceVerificationError, sign_service, verify_service_request,
};

const BODY: &[u8] = br#"{"accountNo":"1231271284141"}"#;
const PATH: &str = "/snap/v1.0/balance-inquiry";
const SECRET: &str = "service-verification-secret";
const TIMESTAMP: &str = "2026-07-28T12:34:56.123+07:00";

#[test]
fn access_token_accepts_rfc6750_b64token_and_scheme_case() {
    for authorization in ["Bearer abc", "bearer abc-._~+/", "BEARER abc==", "bEaReR A0"] {
        assert_eq!(
            AccessToken::parse(authorization).unwrap().as_str(),
            authorization.split_once(' ').unwrap().1,
        );
    }
}

#[test]
fn access_token_rejects_ambiguous_or_non_bearer_inputs() {
    let cases = [
        ("token", AuthorizationError::InvalidSeparator),
        ("Basic token", AuthorizationError::UnsupportedScheme),
        ("Bearer ", AuthorizationError::MissingCredential),
        ("Bearer  token", AuthorizationError::InvalidSeparator),
        ("Bearer token second", AuthorizationError::InvalidSeparator),
        ("Bearer\ttoken", AuthorizationError::InvalidSeparator),
        (" Bearer token", AuthorizationError::UnsupportedScheme),
        ("Bearer token ", AuthorizationError::InvalidSeparator),
        ("Bearer =", AuthorizationError::InvalidCredential),
        ("Bearer ===", AuthorizationError::InvalidCredential),
        ("Bearer ab=c", AuthorizationError::InvalidCredential),
        ("Bearer token:", AuthorizationError::InvalidCredential),
        ("Bearer töken", AuthorizationError::InvalidCredential),
    ];

    for (authorization, expected) in cases {
        assert_eq!(AccessToken::parse(authorization), Err(expected), "{authorization:?}");
    }
}

#[test]
fn service_verification_accepts_matching_request() {
    let authorization = "Bearer access-token";
    let token = AccessToken::parse(authorization).unwrap();
    let canonical = ServiceStringToSign::new(&Method::POST, PATH, token, BODY, TIMESTAMP).unwrap();
    let signature = sign_service(SECRET, &canonical).to_base64();
    let request =
        ServiceRequestParts::new(&Method::POST, PATH, authorization, &signature, TIMESTAMP, BODY).unwrap();

    assert!(verify_service_request(SECRET, request).is_ok());
}

#[test]
fn service_verification_classifies_encoding_and_mismatch() {
    assert!(matches!(
        ServiceRequestParts::new(&Method::POST, PATH, "Bearer access-token", "not base64!", TIMESTAMP, BODY,),
        Err(ServiceVerificationError::InvalidSignatureEncoding),
    ));
    assert!(matches!(
        ServiceRequestParts::new(&Method::POST, PATH, "Bearer access-token", "c2ln", TIMESTAMP, BODY,),
        Err(ServiceVerificationError::InvalidSignatureLength { actual: 3 }),
    ));

    let wrong = kamu_snap_crypto::HmacSigner::new("wrong-secret").sign("wrong-canonical").to_base64();
    let mismatched =
        ServiceRequestParts::new(&Method::POST, PATH, "Bearer access-token", &wrong, TIMESTAMP, BODY)
            .unwrap();
    assert_eq!(verify_service_request(SECRET, mismatched), Err(ServiceVerificationError::SignatureMismatch),);
}

#[test]
fn request_parts_reject_query_and_invalid_timestamp() {
    assert!(matches!(
        ServiceRequestParts::new(&Method::POST, "/p?query=1", "Bearer token", "c2ln", TIMESTAMP, BODY,),
        Err(ServiceVerificationError::Input(InputError::QueryOrFragment)),
    ));
    assert!(matches!(
        ServiceRequestParts::new(&Method::POST, "/p", "Bearer token", "c2ln", "yesterday", BODY,),
        Err(ServiceVerificationError::Input(InputError::InvalidTimestamp)),
    ));
}

#[test]
fn canonical_path_preserves_percent_encoded_octets() {
    let path = "/snap/v1.0/dummy/account%2Falias/%7E";
    assert_eq!(CanonicalPath::parse(path).unwrap().as_str(), path);
}

#[test]
fn request_debug_never_contains_authentication_material_or_body() {
    let signature = kamu_snap_crypto::HmacSigner::new("debug-secret").sign("debug-canonical").to_base64();
    let request =
        ServiceRequestParts::new(&Method::POST, PATH, "Bearer top-secret-token", &signature, TIMESTAMP, BODY)
            .unwrap();
    let debug = format!("{request:?}");
    assert!(!debug.contains("top-secret-token"));
    assert!(!debug.contains(&signature));
    assert!(!debug.contains("accountNo"));
    assert!(debug.contains(&format!("body_len: {}", BODY.len())));
}
