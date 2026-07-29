//! axum/tower inbound-verify glue for SNAP BI service signatures.
//!
//! [`verify_request`] translates `http::request::Parts` into the validated
//! framework-neutral core facade. Buffer limits and body replay remain owned by
//! the application.

#![forbid(unsafe_code)]

use http::request::Parts;
use kamu_snap_crypto::snap_bi::{
    AUTHORIZATION, ServiceRequestParts, ServiceVerificationError, X_SIGNATURE, X_TIMESTAMP,
    verify_service_request,
};

/// Verify a SNAP BI service request against `client_secret`.
///
/// Reads `X-SIGNATURE`, `X-TIMESTAMP`, and `Authorization` from
/// `parts.headers`; uses `parts.method` and `parts.uri.path()` for the
/// canonical stringToSign; hashes the supplied body bytes for the body-hash
/// slot. BRI excludes the URI query from its signature recipe.
pub fn verify_request(
    parts: &Parts,
    body: &[u8],
    client_secret: &str,
) -> Result<(), ServiceVerificationError> {
    let signature_b64 = header_str(&parts.headers, X_SIGNATURE)?;
    let timestamp = header_str(&parts.headers, X_TIMESTAMP)?;
    let authorization = header_str(&parts.headers, AUTHORIZATION)?;
    let request = ServiceRequestParts::new(
        &parts.method,
        parts.uri.path(),
        authorization,
        signature_b64,
        timestamp,
        body,
    )?;
    verify_service_request(client_secret.as_bytes(), request)
}

fn header_str<'a>(
    headers: &'a http::HeaderMap,
    name: &'static str,
) -> Result<&'a str, ServiceVerificationError> {
    headers
        .get(name)
        .ok_or(ServiceVerificationError::MissingHeader { name })?
        .to_str()
        .map_err(|_| ServiceVerificationError::InvalidHeader { name })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::body::{Body, to_bytes};
    use kamu_snap_crypto::HmacSigner;
    use kamu_snap_crypto::snap_bi::{
        AccessToken, ServiceStringToSign, ServiceVerificationError, sha256_lower_hex, sign_service,
    };

    use super::*;

    const BODY: &[u8] = br#"{"amount":"10000.00"}"#;
    const PATH: &str = "/snap/v1.0/transfer-intrabank/payment";
    const SECRET: &str = "adapter-test-secret";
    const TIMESTAMP: &str = "2026-07-28T12:34:56+07:00";

    fn signature(authorization: &str) -> String {
        let token = AccessToken::parse(authorization).unwrap();
        let canonical = ServiceStringToSign::new(&http::Method::POST, PATH, token, BODY, TIMESTAMP).unwrap();
        sign_service(SECRET, &canonical).to_base64()
    }

    fn legacy_signature(token_field: &str) -> String {
        let body_hash = sha256_lower_hex(BODY);
        let canonical = format!("POST:{PATH}:{token_field}:{body_hash}:{TIMESTAMP}");
        HmacSigner::new(SECRET).sign(canonical).to_base64()
    }

    fn parts(uri: &str, authorization: &str, signature: &str) -> Parts {
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(uri)
            .header(AUTHORIZATION, authorization)
            .header(X_SIGNATURE, signature)
            .header(X_TIMESTAMP, TIMESTAMP)
            .body(())
            .unwrap();
        request.into_parts().0
    }

    async fn buffer_then_verify(
        request: http::Request<Body>,
        body_limit: usize,
        verification_calls: &AtomicUsize,
    ) -> Result<(), http::StatusCode> {
        let (parts, body) = request.into_parts();
        let bytes = to_bytes(body, body_limit).await.map_err(|_| http::StatusCode::PAYLOAD_TOO_LARGE)?;
        verification_calls.fetch_add(1, Ordering::Relaxed);
        verify_request(&parts, &bytes, SECRET).map_err(|_| http::StatusCode::UNAUTHORIZED)
    }

    #[test]
    fn accepts_ascii_case_insensitive_bearer_scheme() {
        for authorization in ["Bearer access-token", "bearer access-token", "bEaReR access-token"] {
            let signature = signature(authorization);
            assert!(
                verify_request(&parts(PATH, authorization, &signature), BODY, SECRET).is_ok(),
                "{authorization}",
            );
        }
    }

    #[test]
    fn rejects_non_bearer_and_ambiguous_credentials_before_verification() {
        for authorization in [
            "access-token",
            "Basic access-token",
            "Bearer ",
            "Bearer  access-token",
            "Bearer access-token second",
            "Bearer\taccess-token",
        ] {
            let signature = legacy_signature(authorization);
            assert!(
                matches!(
                    verify_request(&parts(PATH, authorization, &signature), BODY, SECRET,),
                    Err(ServiceVerificationError::Authorization(_))
                ),
                "{authorization:?}",
            );
        }
    }

    #[test]
    fn excludes_query_from_bri_canonical_path() {
        let authorization = "Bearer access-token";
        let signature = signature(authorization);
        let request_parts = parts(&format!("{PATH}?reference=123%20456"), authorization, &signature);
        assert!(verify_request(&request_parts, BODY, SECRET).is_ok());
    }

    #[test]
    fn rejects_missing_header() {
        let request = http::Request::builder().body(()).unwrap();
        assert!(matches!(
            verify_request(&request.into_parts().0, BODY, SECRET),
            Err(ServiceVerificationError::MissingHeader { .. })
        ));
    }

    #[tokio::test]
    async fn oversized_body_never_reaches_verification_as_empty_bytes() {
        let authorization = "Bearer access-token";
        let token = AccessToken::parse(authorization).unwrap();
        let canonical = ServiceStringToSign::new(&http::Method::POST, PATH, token, b"", TIMESTAMP).unwrap();
        let signature = sign_service(SECRET, &canonical).to_base64();
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(PATH)
            .header(AUTHORIZATION, authorization)
            .header(X_SIGNATURE, signature)
            .header(X_TIMESTAMP, TIMESTAMP)
            .body(Body::from("not empty"))
            .unwrap();
        let verification_calls = AtomicUsize::new(0);

        assert_eq!(
            buffer_then_verify(request, 0, &verification_calls).await,
            Err(http::StatusCode::PAYLOAD_TOO_LARGE),
        );
        assert_eq!(verification_calls.load(Ordering::Relaxed), 0);
    }
}
