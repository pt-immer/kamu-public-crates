//! axum/tower inbound-verify glue for SNAP BI service signatures.
//!
//! Provides [`verify_request`] — the framework-agnostic SNAP BI verify
//! function operating on `http::request::Parts` + body bytes. axum's `Parts`
//! gives clean access to method/headers without consuming the body, so
//! consumers can extract via `axum::body::Bytes` (or `axum::body::to_bytes`)
//! and then call this function inside an extractor / handler.
//!
//! A full `tower::Layer` wrapper is intentionally deferred to a v2.x release;
//! body extraction in a layered Service requires careful buffer-and-replay
//! that's better designed once a production caller exists.

#![forbid(unsafe_code)]

use http::request::Parts;
use kamu_snap_crypto::Signature;
use kamu_snap_crypto::snap_bi::{ServiceStringToSign, verify_service};

const X_SIGNATURE: &str = "X-SIGNATURE";
const X_TIMESTAMP: &str = "X-TIMESTAMP";
const AUTHORIZATION: &str = "Authorization";

/// Verify a SNAP BI service request against `client_secret`.
///
/// Reads `X-SIGNATURE`, `X-TIMESTAMP`, and `Authorization` from
/// `parts.headers`; uses `parts.method` and `parts.uri.path()` for the
/// canonical stringToSign; hashes the supplied body bytes for the body-hash
/// slot.
pub fn verify_request(parts: &Parts, body: &[u8], client_secret: &str) -> kamu_snap_crypto::Result<()> {
    let signature_b64 = header_str(&parts.headers, X_SIGNATURE)?;
    let timestamp = header_str(&parts.headers, X_TIMESTAMP)?;
    let authorization = header_str(&parts.headers, AUTHORIZATION)?;
    let access_token = authorization.strip_prefix("Bearer ").unwrap_or(authorization);

    let parts_canonical =
        ServiceStringToSign { method: &parts.method, path: parts.uri.path(), access_token, body, timestamp };

    let sig = Signature::from_base64(signature_b64)?;
    verify_service(client_secret.as_bytes(), &parts_canonical, &sig)
}

fn header_str<'a>(headers: &'a http::HeaderMap, name: &'static str) -> kamu_snap_crypto::Result<&'a str> {
    headers
        .get(name)
        .ok_or_else(|| kamu_snap_crypto::Error::Webhook(format!("missing header: {name}")))?
        .to_str()
        .map_err(|e| kamu_snap_crypto::Error::Webhook(format!("non-ASCII header {name}: {e}")))
}
