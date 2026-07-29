//! SNAP BI canonical header builders.
//!
//! Returns framework-agnostic `Vec<(&'static str, String)>` pairs — wire into
//! `reqwest::HeaderMap`, `actix_web::HttpRequest`, `axum::http::HeaderMap`, or
//! emit directly into a request builder.

use core::fmt;

use http::HeaderValue;

use super::{InputError, SnapTimestamp};

/// Canonical headers for a SNAP BI service request.
#[derive(Clone)]
pub struct ServiceHeaders {
    partner_id: String,
    channel_id: String,
    external_id: String,
    timestamp: String,
    signature: String,
    bearer_token: String,
}

impl ServiceHeaders {
    pub(crate) fn new(
        partner_id: &str,
        channel_id: &str,
        external_id: &str,
        timestamp: &str,
        signature: String,
        bearer_token: &str,
    ) -> Self {
        Self {
            partner_id: partner_id.to_owned(),
            channel_id: channel_id.to_owned(),
            external_id: external_id.to_owned(),
            timestamp: timestamp.to_owned(),
            signature,
            bearer_token: bearer_token.to_owned(),
        }
    }

    /// Render as `(name, value)` pairs for insertion into any HTTP header map.
    ///
    /// Names are the canonical SNAP BI **uppercase** forms (`X-PARTNER-ID`,
    /// `X-SIGNATURE`, …). HTTP header names are case-insensitive, so build the
    /// `HeaderName` with the case-tolerant parser — `HeaderName::from_bytes` or
    /// `TryFrom<&str>` (what `reqwest`/`http` use for `&str` keys). Do **not**
    /// use [`http::HeaderName::from_static`]: it panics on any non-lowercase
    /// input and would reject these names.
    pub fn into_pairs(self) -> Vec<(&'static str, String)> {
        vec![
            ("X-PARTNER-ID", self.partner_id),
            ("CHANNEL-ID", self.channel_id),
            ("X-EXTERNAL-ID", self.external_id),
            ("X-TIMESTAMP", self.timestamp),
            ("X-SIGNATURE", self.signature),
            ("Authorization", format!("Bearer {}", self.bearer_token)),
        ]
    }
}

impl fmt::Debug for ServiceHeaders {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceHeaders")
            .field("partner_id", &"[REDACTED]")
            .field("channel_id", &"[REDACTED]")
            .field("external_id", &"[REDACTED]")
            .field("timestamp", &"[PRESENT]")
            .field("signature", &"[REDACTED]")
            .field("bearer_token", &"[REDACTED]")
            .finish()
    }
}

/// Canonical headers for the SNAP BI OAuth `/access-token/b2b` request.
#[derive(Clone)]
pub struct OAuthHeaders {
    client_key: String,
    timestamp: String,
    signature: String,
}

impl OAuthHeaders {
    /// Validate and construct the OAuth header set.
    pub fn new(client_key: &str, timestamp: &str, signature: &str) -> Result<Self, InputError> {
        validate_header("X-CLIENT-KEY", client_key)?;
        SnapTimestamp::parse(timestamp)?;
        validate_header("X-SIGNATURE", signature)?;
        Ok(Self {
            client_key: client_key.to_owned(),
            timestamp: timestamp.to_owned(),
            signature: signature.to_owned(),
        })
    }

    /// Render as `(name, value)` pairs.
    pub fn into_pairs(self) -> Vec<(&'static str, String)> {
        vec![
            ("X-CLIENT-KEY", self.client_key),
            ("X-TIMESTAMP", self.timestamp),
            ("X-SIGNATURE", self.signature),
        ]
    }
}

impl fmt::Debug for OAuthHeaders {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthHeaders")
            .field("client_key", &"[REDACTED]")
            .field("timestamp", &"[PRESENT]")
            .field("signature", &"[REDACTED]")
            .finish()
    }
}

fn validate_header(name: &'static str, value: &str) -> Result<(), InputError> {
    if value.is_empty() {
        return Err(InputError::EmptyHeaderValue { name });
    }
    HeaderValue::from_str(value).map(|_| ()).map_err(|_| InputError::InvalidHeaderValue { name })
}
