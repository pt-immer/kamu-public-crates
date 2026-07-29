//! Crate-level error taxonomy.

use crate::signature::Encoding;

/// All error conditions surfaced by `kamu-snap-crypto`.
///
/// Marked `#[non_exhaustive]` so adding new variants is non-breaking; consumers
/// matching on this enum must use a wildcard arm.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// SPKI public-key PEM failed to parse. Carries the upstream message.
    #[error("invalid PEM public key: {0}")]
    InvalidPublicKey(String),

    /// PKCS#8 private-key PEM failed to parse. Carries the upstream message.
    #[error("invalid PEM secret key: {0}")]
    InvalidSecretKey(String),

    /// Encoded signature could not be decoded into bytes.
    #[error("signature decode failed ({encoding:?}): {reason}")]
    SignatureDecode {
        /// Encoding that was attempted.
        encoding: Encoding,
        /// Upstream decoder message.
        reason: String,
    },

    /// Raw signature bytes do not have the selected algorithm's wire length or
    /// structure.
    #[error("invalid raw signature: {0}")]
    InvalidRawSignature(String),

    /// HMAC verification failed (signature did not match canonical payload).
    #[error("symmetric verification failed")]
    SymmetricVerifyFailed,

    /// RSA verification failed (signature did not match canonical payload).
    #[error("asymmetric verification failed")]
    AsymmetricVerifyFailed,

    /// Invalid SNAP BI input (feature `snap-bi`).
    #[cfg(feature = "snap-bi")]
    #[error(transparent)]
    SnapBiInput(#[from] crate::snap_bi::InputError),

    /// SNAP BI service-request verification failed (feature `snap-bi`).
    #[cfg(feature = "snap-bi")]
    #[error(transparent)]
    ServiceVerification(#[from] crate::snap_bi::ServiceVerificationError),

    /// A required HTTP header was absent.
    #[cfg(any(feature = "snap-bi", feature = "webhook"))]
    #[error("missing required header {name}")]
    MissingHeader {
        /// Canonical header name.
        name: &'static str,
    },

    /// An HTTP header was not valid visible ASCII.
    #[cfg(any(feature = "snap-bi", feature = "webhook"))]
    #[error("invalid value for header {name}")]
    InvalidHeader {
        /// Canonical header name.
        name: &'static str,
    },
}

/// Shorthand for `core::result::Result<T, crate::Error>`.
pub type Result<T> = core::result::Result<T, Error>;
