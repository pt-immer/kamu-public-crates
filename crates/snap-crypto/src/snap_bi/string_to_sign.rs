//! Canonical SNAP BI `stringToSign` builders.
//!
//! Two recipes are encoded:
//!
//! - [`ServiceStringToSign`] — the colon-separated form used by every SNAP BI
//!   service endpoint:
//!   `HTTPMethod:relativePath:accessToken:lowercaseHex(SHA256(minifiedBody)):
//!   timestamp`.
//! - [`OAuthStringToSign`] — the form used by `/access-token/b2b` OAuth:
//!   `clientId|timestamp`.
//!
//! Both builders accept the minified body as a byte slice; the caller is
//! responsible for JSON minification (consumer's serde already minifies by
//! default).

use http::Method;

use super::hash::sha256_lower_hex;

/// Inputs for the SNAP BI service `stringToSign`.
///
/// All fields are borrowed. The builder validates nothing — the caller is
/// expected to have already constructed valid SNAP BI inputs (method, path,
/// access-token, body, timestamp).
#[derive(Debug, Clone)]
pub struct ServiceStringToSign<'a> {
    /// HTTP method.
    pub method: &'a Method,
    /// Path part of the request URL (origin-form, starts with `/`).
    pub path: &'a str,
    /// Bearer access token issued by the upstream OAuth flow.
    pub access_token: &'a str,
    /// Already-minified request body bytes.
    pub body: &'a [u8],
    /// ISO-8601 Jakarta timestamp (must match the `X-TIMESTAMP` header
    /// byte-for-byte).
    pub timestamp: &'a str,
}

impl ServiceStringToSign<'_> {
    /// Build the canonical service-endpoint `stringToSign`.
    pub fn build(&self) -> String {
        let body_hash = sha256_lower_hex(self.body);
        format!(
            "{method}:{path}:{token}:{body_hash}:{ts}",
            method = self.method.as_str(),
            path = self.path,
            token = self.access_token,
            body_hash = body_hash,
            ts = self.timestamp,
        )
    }
}

/// Inputs for the SNAP BI OAuth `stringToSign`.
#[derive(Debug, Clone)]
pub struct OAuthStringToSign<'a> {
    /// `clientId` issued by the partner during onboarding.
    pub client_id: &'a str,
    /// ISO-8601 Jakarta timestamp (must match the `X-TIMESTAMP` header).
    pub timestamp: &'a str,
}

impl OAuthStringToSign<'_> {
    /// Build the canonical OAuth `stringToSign`.
    pub fn build(&self) -> String {
        format!("{}|{}", self.client_id, self.timestamp)
    }
}
