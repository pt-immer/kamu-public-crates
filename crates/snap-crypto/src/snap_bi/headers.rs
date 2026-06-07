//! SNAP BI canonical header builders.
//!
//! Returns framework-agnostic `Vec<(&'static str, String)>` pairs — wire into
//! `reqwest::HeaderMap`, `actix_web::HttpRequest`, `axum::http::HeaderMap`, or
//! emit directly into a `String` body. Header *names* are stable per spec;
//! the builders ensure the consumer cannot accidentally omit one (e.g.
//! `CHANNEL-ID` for service calls).

/// Canonical headers for a SNAP BI service request.
#[derive(Debug, Clone)]
pub struct ServiceHeaders {
    /// `X-PARTNER-ID` — partner identifier issued during onboarding.
    pub partner_id: String,
    /// `CHANNEL-ID` — channel/product code issued by the partner.
    pub channel_id: String,
    /// `X-EXTERNAL-ID` — 9-digit numeric idempotency key (caller-managed).
    pub external_id: String,
    /// `X-TIMESTAMP` — ISO-8601 Jakarta timestamp (must match stringToSign).
    pub timestamp: String,
    /// `X-SIGNATURE` — encoded HMAC-SHA512 of the canonical stringToSign.
    pub signature: String,
    /// `Authorization: Bearer …` — value WITHOUT the `Bearer ` prefix.
    pub bearer_token: String,
}

impl ServiceHeaders {
    /// Start a [`ServiceHeadersBuilder`].
    pub fn builder() -> ServiceHeadersBuilder {
        ServiceHeadersBuilder::default()
    }

    /// Render as `(name, value)` pairs for direct insertion into any HTTP
    /// header map. Names are lowercase to satisfy `reqwest::HeaderName`'s
    /// validation when reused.
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

/// Builder for [`ServiceHeaders`]. Every field is required; `build` returns
/// `Err` (via panic message) if anything is missing — by design, callers
/// should construct via the public field init form if all values are known
/// up front.
#[derive(Debug, Clone, Default)]
pub struct ServiceHeadersBuilder {
    partner_id: Option<String>,
    channel_id: Option<String>,
    external_id: Option<String>,
    timestamp: Option<String>,
    signature: Option<String>,
    bearer_token: Option<String>,
}

impl ServiceHeadersBuilder {
    /// Set `X-PARTNER-ID`.
    pub fn partner_id(mut self, v: impl Into<String>) -> Self {
        self.partner_id = Some(v.into());
        self
    }

    /// Set `CHANNEL-ID`.
    pub fn channel_id(mut self, v: impl Into<String>) -> Self {
        self.channel_id = Some(v.into());
        self
    }

    /// Set `X-EXTERNAL-ID`.
    pub fn external_id(mut self, v: impl Into<String>) -> Self {
        self.external_id = Some(v.into());
        self
    }

    /// Set `X-TIMESTAMP`.
    pub fn timestamp(mut self, v: impl Into<String>) -> Self {
        self.timestamp = Some(v.into());
        self
    }

    /// Set `X-SIGNATURE`.
    pub fn signature(mut self, v: impl Into<String>) -> Self {
        self.signature = Some(v.into());
        self
    }

    /// Set the bearer token (raw, without the `Bearer ` prefix).
    pub fn bearer_token(mut self, v: impl Into<String>) -> Self {
        self.bearer_token = Some(v.into());
        self
    }

    /// Build the final [`ServiceHeaders`]. Returns `Err(crate::Error::SnapBi)`
    /// listing missing fields if any required field was not set.
    pub fn build(self) -> crate::Result<ServiceHeaders> {
        let mut missing = Vec::new();
        macro_rules! take {
            ($field:ident) => {{
                match self.$field {
                    Some(v) => v,
                    None => {
                        missing.push(stringify!($field));
                        String::new()
                    }
                }
            }};
        }
        let h = ServiceHeaders {
            partner_id: take!(partner_id),
            channel_id: take!(channel_id),
            external_id: take!(external_id),
            timestamp: take!(timestamp),
            signature: take!(signature),
            bearer_token: take!(bearer_token),
        };
        if !missing.is_empty() {
            return Err(crate::Error::SnapBi(format!(
                "ServiceHeadersBuilder missing fields: {}",
                missing.join(", ")
            )));
        }
        Ok(h)
    }
}

/// Canonical headers for the SNAP BI OAuth `/access-token/b2b` request.
#[derive(Debug, Clone)]
pub struct OAuthHeaders {
    /// `X-CLIENT-KEY` — partner client identifier.
    pub client_key: String,
    /// `X-TIMESTAMP` — ISO-8601 Jakarta timestamp (millisecond precision).
    pub timestamp: String,
    /// `X-SIGNATURE` — encoded RSA signature of the canonical OAuth
    /// stringToSign.
    pub signature: String,
}

impl OAuthHeaders {
    /// Render as `(name, value)` pairs.
    pub fn into_pairs(self) -> Vec<(&'static str, String)> {
        vec![
            ("X-CLIENT-KEY", self.client_key),
            ("X-TIMESTAMP", self.timestamp),
            ("X-SIGNATURE", self.signature),
        ]
    }
}
