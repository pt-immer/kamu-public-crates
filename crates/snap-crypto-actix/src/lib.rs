//! actix-web inbound-verify glue for SNAP BI service signatures.
//!
//! [`verify_request`] translates Actix request types into the validated
//! framework-neutral core facade. Buffer limits and body replay remain owned by
//! the application.

#![forbid(unsafe_code)]

use actix_web::http::{Method as ActixMethod, header::HeaderMap as ActixHeaderMap};
use kamu_snap_crypto::snap_bi::{
    AUTHORIZATION, ServiceRequestParts, ServiceVerificationError, X_SIGNATURE, X_TIMESTAMP,
    verify_service_request,
};

/// Verify a SNAP BI service request against `client_secret`. Returns `Ok(())`
/// when `X-SIGNATURE` matches the canonical stringToSign computed from the
/// supplied method, path, body, `X-TIMESTAMP` header, and Bearer access
/// token. Any missing header or signature mismatch yields a
/// [`ServiceVerificationError`].
pub fn verify_request(
    method: &ActixMethod,
    path: &str,
    headers: &ActixHeaderMap,
    body: &[u8],
    client_secret: &str,
) -> Result<(), ServiceVerificationError> {
    let signature_b64 = header_str(headers, X_SIGNATURE)?;
    let timestamp = header_str(headers, X_TIMESTAMP)?;
    let authorization = header_str(headers, AUTHORIZATION)?;
    let http_method = http::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| ServiceVerificationError::InvalidMethod)?;

    let request =
        ServiceRequestParts::new(&http_method, path, authorization, signature_b64, timestamp, body)?;
    verify_service_request(client_secret.as_bytes(), request)
}

fn header_str<'a>(
    headers: &'a ActixHeaderMap,
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
    use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
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
        let method = http::Method::POST;
        let canonical = ServiceStringToSign::new(&method, PATH, token, BODY, TIMESTAMP).unwrap();
        sign_service(SECRET, &canonical).to_base64()
    }

    fn legacy_signature(token_field: &str) -> String {
        let body_hash = sha256_lower_hex(BODY);
        let canonical = format!("POST:{PATH}:{token_field}:{body_hash}:{TIMESTAMP}");
        HmacSigner::new(SECRET).sign(canonical).to_base64()
    }

    fn headers(authorization: &str, signature: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in
            [("authorization", authorization), ("x-signature", signature), ("x-timestamp", TIMESTAMP)]
        {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn accepts_ascii_case_insensitive_bearer_scheme() {
        for authorization in ["Bearer access-token", "bearer access-token", "bEaReR access-token"] {
            let signature = signature(authorization);
            assert!(
                verify_request(&ActixMethod::POST, PATH, &headers(authorization, &signature), BODY, SECRET,)
                    .is_ok(),
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
                    verify_request(
                        &ActixMethod::POST,
                        PATH,
                        &headers(authorization, &signature),
                        BODY,
                        SECRET,
                    ),
                    Err(ServiceVerificationError::Authorization(_))
                ),
                "{authorization:?}",
            );
        }
    }

    #[test]
    fn rejects_missing_header() {
        assert!(matches!(
            verify_request(&ActixMethod::POST, PATH, &HeaderMap::new(), BODY, SECRET,),
            Err(ServiceVerificationError::MissingHeader { .. })
        ));
    }
}
