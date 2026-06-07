//! actix-web inbound-verify glue for SNAP BI service signatures.
//!
//! Provides [`verify_request`] — a single function that takes the parts of an
//! `actix-web` request (method, path, headers, body) plus a client secret,
//! and returns `Ok(())` iff the incoming `X-SIGNATURE` validates against the
//! canonical SNAP BI service `stringToSign`.
//!
//! A full `Transform`/middleware wrapper is intentionally deferred to a v2.x
//! release — body extraction inside actix middleware requires
//! buffer-and-replay plumbing that's better designed once we have a
//! production caller. For now, consumers call [`verify_request`] from inside
//! their own handler (or a custom `FromRequest` extractor) after the body is
//! materialised.

#![forbid(unsafe_code)]

use actix_web::http::{Method as ActixMethod, header::HeaderMap as ActixHeaderMap};
use kamu_snap_crypto::Signature;
use kamu_snap_crypto::snap_bi::{ServiceStringToSign, verify_service};

const X_SIGNATURE: &str = "X-SIGNATURE";
const X_TIMESTAMP: &str = "X-TIMESTAMP";
const AUTHORIZATION: &str = "Authorization";

/// Verify a SNAP BI service request against `client_secret`. Returns `Ok(())`
/// when `X-SIGNATURE` matches the canonical stringToSign computed from the
/// supplied method, path, body, `X-TIMESTAMP` header, and Bearer access
/// token. Any missing header or signature mismatch yields a
/// [`kamu_snap_crypto::Error`].
pub fn verify_request(
    method: &ActixMethod,
    path: &str,
    headers: &ActixHeaderMap,
    body: &[u8],
    client_secret: &str,
) -> kamu_snap_crypto::Result<()> {
    let signature_b64 = header_str(headers, X_SIGNATURE)?;
    let timestamp = header_str(headers, X_TIMESTAMP)?;
    let authorization = header_str(headers, AUTHORIZATION)?;
    let access_token = authorization.strip_prefix("Bearer ").unwrap_or(authorization);

    let http_method = http::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| kamu_snap_crypto::Error::Webhook(format!("invalid HTTP method: {e}")))?;

    let parts = ServiceStringToSign { method: &http_method, path, access_token, body, timestamp };

    let sig = Signature::from_base64(signature_b64)?;
    verify_service(client_secret.as_bytes(), &parts, &sig)
}

fn header_str<'a>(headers: &'a ActixHeaderMap, name: &'static str) -> kamu_snap_crypto::Result<&'a str> {
    headers
        .get(name)
        .ok_or_else(|| kamu_snap_crypto::Error::Webhook(format!("missing header: {name}")))?
        .to_str()
        .map_err(|e| kamu_snap_crypto::Error::Webhook(format!("non-ASCII header {name}: {e}")))
}
