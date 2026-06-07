//! Encoding-agnostic signature bytes.
//!
//! Sign operations return [`Signature`] — a thin newtype over `Vec<u8>` — and
//! the caller picks the wire encoding via [`Signature::to_base64`],
//! [`Signature::to_hex_lower`], or [`Signature::encode`]. Verify operations
//! decode from the same set via [`Signature::from_base64`] etc.

use base64::Engine;

use crate::error::{Error, Result};

/// Raw signature bytes. Convert to/from the chosen wire encoding via the
/// `to_*` / `from_*` methods, or the dispatcher [`Signature::encode`] /
/// [`Signature::decode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(Vec<u8>);

/// Supported signature wire encodings.
///
/// `#[non_exhaustive]` so future encodings (e.g. base32, base58) can be added
/// without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Encoding {
    /// Standard base64 with padding (RFC 4648 §4). The SNAP BI default.
    Base64,
    /// URL-safe base64 without padding (RFC 4648 §5).
    Base64UrlNoPad,
    /// Lowercase hex (RFC 4648 §8 with lowercase alphabet). Used by some
    /// BRI doc-prescribed flows and many non-SNAP webhook providers.
    HexLower,
}

impl Signature {
    /// Construct from raw bytes.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Borrow the underlying bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume and return the underlying bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Encode as standard base64 (with padding).
    pub fn to_base64(&self) -> String {
        base64::prelude::BASE64_STANDARD.encode(&self.0)
    }

    /// Encode as URL-safe base64 without padding.
    pub fn to_base64_url_nopad(&self) -> String {
        base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(&self.0)
    }

    /// Encode as lowercase hex.
    pub fn to_hex_lower(&self) -> String {
        hex::encode(&self.0)
    }

    /// Encode using the given [`Encoding`].
    pub fn encode(&self, enc: Encoding) -> String {
        match enc {
            Encoding::Base64 => self.to_base64(),
            Encoding::Base64UrlNoPad => self.to_base64_url_nopad(),
            Encoding::HexLower => self.to_hex_lower(),
        }
    }

    /// Decode from standard base64.
    pub fn from_base64(s: impl AsRef<str>) -> Result<Self> {
        base64::prelude::BASE64_STANDARD
            .decode(s.as_ref())
            .map(Self)
            .map_err(|e| Error::SignatureDecode { encoding: Encoding::Base64, reason: e.to_string() })
    }

    /// Decode from URL-safe base64 without padding.
    pub fn from_base64_url_nopad(s: impl AsRef<str>) -> Result<Self> {
        base64::prelude::BASE64_URL_SAFE_NO_PAD
            .decode(s.as_ref())
            .map(Self)
            .map_err(|e| Error::SignatureDecode { encoding: Encoding::Base64UrlNoPad, reason: e.to_string() })
    }

    /// Decode from lowercase or uppercase hex.
    pub fn from_hex(s: impl AsRef<str>) -> Result<Self> {
        hex::decode(s.as_ref())
            .map(Self)
            .map_err(|e| Error::SignatureDecode { encoding: Encoding::HexLower, reason: e.to_string() })
    }

    /// Decode using the given [`Encoding`].
    pub fn decode(s: impl AsRef<str>, enc: Encoding) -> Result<Self> {
        match enc {
            Encoding::Base64 => Self::from_base64(s),
            Encoding::Base64UrlNoPad => Self::from_base64_url_nopad(s),
            Encoding::HexLower => Self::from_hex(s),
        }
    }
}
