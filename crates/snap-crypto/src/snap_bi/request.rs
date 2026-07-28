//! Validated SNAP BI service requests.

use core::fmt;

use http::{HeaderValue, Method};

use crate::{HmacSigner, Signature};

use super::{ServiceHeaders, ServiceStringToSign};

/// Canonical SNAP BI request-header names.
pub const AUTHORIZATION: &str = "Authorization";
/// Canonical SNAP BI signature-header name.
pub const X_SIGNATURE: &str = "X-SIGNATURE";
/// Canonical SNAP BI timestamp-header name.
pub const X_TIMESTAMP: &str = "X-TIMESTAMP";

const HMAC_SHA512_SIGNATURE_BYTES: usize = 64;

/// Why an `Authorization` value is not exactly one Bearer credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuthorizationError {
    /// The authentication scheme is not `Bearer` (ASCII case-insensitive).
    #[error("authorization scheme must be Bearer")]
    UnsupportedScheme,
    /// Scheme and credential are not separated by exactly one ASCII space.
    #[error("authorization must contain exactly one ASCII space")]
    InvalidSeparator,
    /// The Bearer credential is empty.
    #[error("Bearer credential must not be empty")]
    MissingCredential,
    /// The credential is not an RFC 6750 `b64token`.
    #[error("Bearer credential contains invalid bytes")]
    InvalidCredential,
}

/// A parsed Bearer credential.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AccessToken<'a>(&'a str);

impl<'a> AccessToken<'a> {
    /// Parse `Bearer <credential>`.
    ///
    /// The scheme is ASCII case-insensitive. The separator is exactly one
    /// ASCII space and the credential follows RFC 6750's `b64token` grammar.
    pub fn parse(authorization: &'a str) -> Result<Self, AuthorizationError> {
        let Some((scheme, credential)) = authorization.split_once(' ') else {
            return Err(AuthorizationError::InvalidSeparator);
        };
        if !scheme.eq_ignore_ascii_case("Bearer") {
            return Err(AuthorizationError::UnsupportedScheme);
        }
        if credential.is_empty() {
            return Err(AuthorizationError::MissingCredential);
        }
        if credential.chars().any(|character| character.is_ascii_whitespace()) {
            return Err(AuthorizationError::InvalidSeparator);
        }
        Self::from_credential(credential)
    }

    /// Validate a raw credential for an outbound request.
    pub fn from_credential(credential: &'a str) -> Result<Self, AuthorizationError> {
        if credential.is_empty() {
            return Err(AuthorizationError::MissingCredential);
        }

        let mut padding = false;
        let mut token_byte_seen = false;
        for byte in credential.bytes() {
            if byte == b'=' {
                padding = true;
                continue;
            }
            let allowed =
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/');
            if padding || !allowed {
                return Err(AuthorizationError::InvalidCredential);
            }
            token_byte_seen = true;
        }
        if !token_byte_seen {
            return Err(AuthorizationError::InvalidCredential);
        }

        Ok(Self(credential))
    }

    /// Return the credential without the `Bearer ` prefix.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl fmt::Debug for AccessToken<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("AccessToken").field(&"[REDACTED]").finish()
    }
}

/// Why a SNAP BI request component is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InputError {
    /// A canonical path must be non-empty origin-form.
    #[error("SNAP BI path must be origin-form and start with '/'")]
    InvalidPath,
    /// BRI's SNAP BI recipe excludes query and fragment components.
    #[error("SNAP BI canonical path must not contain a query or fragment")]
    QueryOrFragment,
    /// Timestamp is not RFC 3339 / ISO 8601.
    #[error("SNAP BI timestamp must be RFC 3339")]
    InvalidTimestamp,
    /// External ID is not exactly nine ASCII digits.
    #[error("X-EXTERNAL-ID must contain exactly nine ASCII digits")]
    InvalidExternalId,
    /// A required header value is empty.
    #[error("{name} must not be empty")]
    EmptyHeaderValue {
        /// Canonical header name.
        name: &'static str,
    },
    /// A value cannot be represented by `http::HeaderValue`.
    #[error("{name} is not a valid HTTP header value")]
    InvalidHeaderValue {
        /// Canonical header name.
        name: &'static str,
    },
}

/// Origin-form path used by the BRI SNAP BI signature recipe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CanonicalPath<'a>(&'a str);

impl<'a> CanonicalPath<'a> {
    /// Validate an origin-form path.
    ///
    /// Percent-encoded octets are preserved byte-for-byte. Query and fragment
    /// components are rejected because BRI excludes them from `stringToSign`.
    pub fn parse(path: &'a str) -> Result<Self, InputError> {
        if path.is_empty() || !path.starts_with('/') {
            return Err(InputError::InvalidPath);
        }
        if path.contains(['?', '#']) {
            return Err(InputError::QueryOrFragment);
        }
        Ok(Self(path))
    }

    /// Borrow the validated path.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl fmt::Debug for CanonicalPath<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("CanonicalPath").field(&self.0).finish()
    }
}

/// Validated RFC 3339 timestamp.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SnapTimestamp<'a>(&'a str);

impl<'a> SnapTimestamp<'a> {
    /// Parse an RFC 3339 timestamp accepted by SNAP BI.
    pub fn parse(timestamp: &'a str) -> Result<Self, InputError> {
        chrono::DateTime::parse_from_rfc3339(timestamp).map_err(|_| InputError::InvalidTimestamp)?;
        Ok(Self(timestamp))
    }

    /// Borrow the original timestamp.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl fmt::Debug for SnapTimestamp<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SnapTimestamp").field(&"[VALIDATED]").finish()
    }
}

/// Validated nine-digit `X-EXTERNAL-ID`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExternalId<'a>(&'a str);

impl<'a> ExternalId<'a> {
    /// Validate exactly nine ASCII digits.
    pub fn parse(external_id: &'a str) -> Result<Self, InputError> {
        if external_id.len() != 9 || !external_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InputError::InvalidExternalId);
        }
        Ok(Self(external_id))
    }

    /// Borrow the validated ID.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl fmt::Debug for ExternalId<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ExternalId").field(&"[VALIDATED]").finish()
    }
}

/// Failure while translating or verifying an inbound service request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ServiceVerificationError {
    /// A required header is absent.
    #[error("missing required header {name}")]
    MissingHeader {
        /// Canonical header name.
        name: &'static str,
    },
    /// A header is not visible ASCII.
    #[error("invalid value for header {name}")]
    InvalidHeader {
        /// Canonical header name.
        name: &'static str,
    },
    /// Framework method could not be translated to `http::Method`.
    #[error("invalid HTTP method")]
    InvalidMethod,
    /// Authorization grammar failed.
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
    /// Another canonical input failed validation.
    #[error(transparent)]
    Input(#[from] InputError),
    /// `X-SIGNATURE` is not standard padded base64.
    #[error("X-SIGNATURE is not valid standard base64")]
    InvalidSignatureEncoding,
    /// Decoded `X-SIGNATURE` is not one HMAC-SHA512 output.
    #[error("X-SIGNATURE must decode to 64 bytes, got {actual}")]
    InvalidSignatureLength {
        /// Decoded byte length.
        actual: usize,
    },
    /// Signature does not authenticate the canonical request.
    #[error("SNAP BI service signature mismatch")]
    SignatureMismatch,
}

/// Validated framework-neutral inbound request.
#[derive(Clone)]
pub struct ServiceRequestParts<'a> {
    method: &'a Method,
    path: CanonicalPath<'a>,
    access_token: AccessToken<'a>,
    signature: Signature,
    timestamp: SnapTimestamp<'a>,
    body: &'a [u8],
}

impl<'a> ServiceRequestParts<'a> {
    /// Validate raw request values at the framework boundary.
    pub fn new(
        method: &'a Method,
        path: &'a str,
        authorization: &'a str,
        signature: &'a str,
        timestamp: &'a str,
        body: &'a [u8],
    ) -> Result<Self, ServiceVerificationError> {
        let path = CanonicalPath::parse(path)?;
        let access_token = AccessToken::parse(authorization)?;
        let timestamp = SnapTimestamp::parse(timestamp)?;
        let signature = Signature::from_base64(signature)
            .map_err(|_| ServiceVerificationError::InvalidSignatureEncoding)?;
        if signature.as_bytes().len() != HMAC_SHA512_SIGNATURE_BYTES {
            return Err(ServiceVerificationError::InvalidSignatureLength {
                actual: signature.as_bytes().len(),
            });
        }
        Ok(Self { method, path, access_token, signature, timestamp, body })
    }

    fn canonical(&self) -> ServiceStringToSign<'a> {
        ServiceStringToSign::from_validated(
            self.method,
            self.path,
            self.access_token,
            self.body,
            self.timestamp,
        )
    }
}

impl fmt::Debug for ServiceRequestParts<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceRequestParts")
            .field("method", self.method)
            .field("path", &self.path)
            .field("access_token", &"[REDACTED]")
            .field("signature", &"[REDACTED]")
            .field("timestamp", &"[PRESENT]")
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// Verify an inbound service request with HMAC-SHA512.
pub fn verify_service_request(
    client_secret: impl AsRef<[u8]>,
    request: ServiceRequestParts<'_>,
) -> Result<(), ServiceVerificationError> {
    let signer = HmacSigner::new(client_secret);
    verify_service_request_with(&signer, request)
}

pub(crate) fn verify_service_request_with(
    signer: &HmacSigner,
    request: ServiceRequestParts<'_>,
) -> Result<(), ServiceVerificationError> {
    signer
        .verify(&request.signature, request.canonical().build())
        .map_err(|_| ServiceVerificationError::SignatureMismatch)
}

/// Marker for an outbound request that has not been signed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unsigned;

/// State carried by an authenticated outbound request.
#[derive(Clone)]
pub struct Signed {
    signature: Signature,
}

/// Outbound request whose available operations depend on signing state.
pub struct ServiceRequest<'a, State> {
    canonical: ServiceStringToSign<'a>,
    partner_id: &'a str,
    channel_id: &'a str,
    external_id: ExternalId<'a>,
    state: State,
}

impl<'a> ServiceRequest<'a, Unsigned> {
    /// Construct a validated unsigned request.
    pub fn new(
        canonical: ServiceStringToSign<'a>,
        partner_id: &'a str,
        channel_id: &'a str,
        external_id: &'a str,
    ) -> Result<Self, InputError> {
        validate_header("X-PARTNER-ID", partner_id)?;
        validate_header("CHANNEL-ID", channel_id)?;
        let external_id = ExternalId::parse(external_id)?;
        Ok(Self { canonical, partner_id, channel_id, external_id, state: Unsigned })
    }

    /// Authenticate the canonical request and transition to [`Signed`].
    #[must_use]
    pub fn sign(self, signer: &HmacSigner) -> ServiceRequest<'a, Signed> {
        let signature = signer.sign(self.canonical.build());
        ServiceRequest {
            canonical: self.canonical,
            partner_id: self.partner_id,
            channel_id: self.channel_id,
            external_id: self.external_id,
            state: Signed { signature },
        }
    }
}

impl ServiceRequest<'_, Signed> {
    /// Build the complete wire header set.
    #[must_use]
    pub fn headers(&self) -> ServiceHeaders {
        ServiceHeaders::new(
            self.partner_id,
            self.channel_id,
            self.external_id.as_str(),
            self.canonical.timestamp().as_str(),
            self.state.signature.to_base64(),
            self.canonical.access_token().as_str(),
        )
    }

    /// Borrow the raw HMAC signature.
    #[must_use]
    pub const fn signature(&self) -> &Signature {
        &self.state.signature
    }
}

impl<State> fmt::Debug for ServiceRequest<'_, State> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceRequest")
            .field("method", self.canonical.method())
            .field("path", &self.canonical.path())
            .field("access_token", &"[REDACTED]")
            .field("timestamp", &"[PRESENT]")
            .field("body_len", &self.canonical.body().len())
            .field("partner_id", &"[PRESENT]")
            .field("channel_id", &"[PRESENT]")
            .field("external_id", &"[VALIDATED]")
            .field("signature", &"[REDACTED OR ABSENT]")
            .finish()
    }
}

fn validate_header(name: &'static str, value: &str) -> Result<(), InputError> {
    if value.is_empty() {
        return Err(InputError::EmptyHeaderValue { name });
    }
    HeaderValue::from_str(value).map(|_| ()).map_err(|_| InputError::InvalidHeaderValue { name })
}
