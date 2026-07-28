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
use super::request::{AccessToken, CanonicalPath, InputError, SnapTimestamp};

/// Inputs for the SNAP BI service `stringToSign`.
///
/// Construction validates path and timestamp syntax. The access token must
/// already have passed [`AccessToken`] validation.
#[derive(Clone)]
pub struct ServiceStringToSign<'a> {
    method: &'a Method,
    path: CanonicalPath<'a>,
    access_token: AccessToken<'a>,
    body: &'a [u8],
    timestamp: SnapTimestamp<'a>,
}

impl<'a> ServiceStringToSign<'a> {
    /// Validate inputs for a service `stringToSign`.
    pub fn new(
        method: &'a Method,
        path: &'a str,
        access_token: AccessToken<'a>,
        body: &'a [u8],
        timestamp: &'a str,
    ) -> Result<Self, InputError> {
        Ok(Self {
            method,
            path: CanonicalPath::parse(path)?,
            access_token,
            body,
            timestamp: SnapTimestamp::parse(timestamp)?,
        })
    }

    pub(crate) const fn from_validated(
        method: &'a Method,
        path: CanonicalPath<'a>,
        access_token: AccessToken<'a>,
        body: &'a [u8],
        timestamp: SnapTimestamp<'a>,
    ) -> Self {
        Self { method, path, access_token, body, timestamp }
    }

    /// Build the canonical service-endpoint `stringToSign`.
    #[must_use]
    pub fn build(&self) -> String {
        let body_hash = sha256_lower_hex(self.body);
        format!(
            "{method}:{path}:{token}:{body_hash}:{ts}",
            method = self.method.as_str(),
            path = self.path.as_str(),
            token = self.access_token.as_str(),
            body_hash = body_hash,
            ts = self.timestamp.as_str(),
        )
    }

    pub(crate) const fn method(&self) -> &Method {
        self.method
    }

    pub(crate) const fn path(&self) -> CanonicalPath<'_> {
        self.path
    }

    pub(crate) const fn access_token(&self) -> AccessToken<'_> {
        self.access_token
    }

    pub(crate) const fn body(&self) -> &[u8] {
        self.body
    }

    pub(crate) const fn timestamp(&self) -> SnapTimestamp<'_> {
        self.timestamp
    }
}

impl core::fmt::Debug for ServiceStringToSign<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ServiceStringToSign")
            .field("method", self.method)
            .field("path", &self.path)
            .field("access_token", &"[REDACTED]")
            .field("body_len", &self.body.len())
            .field("timestamp", &"[PRESENT]")
            .finish()
    }
}

/// Inputs for the SNAP BI OAuth `stringToSign`.
#[derive(Clone)]
pub struct OAuthStringToSign<'a> {
    client_id: &'a str,
    timestamp: SnapTimestamp<'a>,
}

impl<'a> OAuthStringToSign<'a> {
    /// Validate OAuth canonical inputs.
    pub fn new(client_id: &'a str, timestamp: &'a str) -> Result<Self, InputError> {
        if client_id.is_empty() {
            return Err(InputError::EmptyHeaderValue { name: "X-CLIENT-KEY" });
        }
        http::HeaderValue::from_str(client_id)
            .map_err(|_| InputError::InvalidHeaderValue { name: "X-CLIENT-KEY" })?;
        Ok(Self { client_id, timestamp: SnapTimestamp::parse(timestamp)? })
    }

    /// Build the canonical OAuth `stringToSign`.
    #[must_use]
    pub fn build(&self) -> String {
        format!("{}|{}", self.client_id, self.timestamp.as_str())
    }
}

impl core::fmt::Debug for OAuthStringToSign<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("OAuthStringToSign")
            .field("client_id", &"[PRESENT]")
            .field("timestamp", &"[PRESENT]")
            .finish()
    }
}
