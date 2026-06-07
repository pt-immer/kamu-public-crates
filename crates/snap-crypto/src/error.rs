//! Crate-level error taxonomy.

use crate::signature::Encoding;

/// All error conditions surfaced by `kamu-snap-crypto`.
///
/// Marked `#[non_exhaustive]` so adding new variants is non-breaking; consumers
/// matching on this enum must use a wildcard arm.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// PKCS#8 public-key PEM failed to parse. Carries the upstream message.
    #[error("invalid PEM public key: {0}")]
    InvalidPublicKey(String),

    /// PKCS#8 private-key PEM failed to parse. Carries the upstream message.
    #[error("invalid PEM secret key: {0}")]
    InvalidSecretKey(String),

    /// HMAC secret length rejected by the underlying primitive.
    #[error("invalid HMAC secret length")]
    InvalidSecretLength,

    /// Encoded signature could not be decoded into bytes.
    #[error("signature decode failed ({encoding:?}): {reason}")]
    SignatureDecode {
        /// Encoding that was attempted.
        encoding: Encoding,
        /// Upstream decoder message.
        reason: String,
    },

    /// HMAC verification failed (signature did not match canonical payload).
    #[error("symmetric verification failed")]
    SymmetricVerifyFailed,

    /// RSA verification failed (signature did not match canonical payload).
    #[error("asymmetric verification failed")]
    AsymmetricVerifyFailed,

    /// `snap_bi` recipe layer error (feature `snap-bi`).
    #[cfg(feature = "snap-bi")]
    #[error("snap-bi recipe error: {0}")]
    SnapBi(String),

    /// Webhook verifier error (feature `webhook`).
    #[cfg(feature = "webhook")]
    #[error("webhook verifier error: {0}")]
    Webhook(String),
}

/// Shorthand for `core::result::Result<T, crate::Error>`.
pub type Result<T> = core::result::Result<T, Error>;
