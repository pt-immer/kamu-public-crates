//! HMAC-SHA512 sign/verify.
//!
//! [`HmacSigner`] holds the initialised HMAC state internally; `sign` and
//! `verify` take `&self` and clone the state per call (HMAC is cheap to
//! `Clone`). This lets a single signer be shared across threads without a
//! `Mutex` while still letting consumers compute many signatures from one
//! initialised key.
//!
//! SNAP BI mandates HMAC-SHA512; if a future variant requires SHA-256 or
//! SHA-3, the crate will gain a sibling `HmacSigner<D>` generic. Today the
//! ergonomic tradeoff favours the fixed-digest form.

use ::hmac::{Hmac, Mac};
use sha2::Sha512;

use crate::error::{Error, Result};
use crate::signature::Signature;

/// Stateful HMAC-SHA512 signer/verifier.
///
/// Verification uses [`Mac::verify_slice`], which is a constant-time
/// comparison by upstream guarantee.
#[derive(Clone)]
pub struct HmacSigner {
    inner: Hmac<Sha512>,
}

impl HmacSigner {
    /// Initialise from a raw secret. Any byte slice is accepted by HMAC-SHA512;
    /// this constructor only fails if the upstream primitive rejects the key
    /// (current `hmac` 0.12 never does, but the error is surfaced for
    /// forward-compatibility).
    pub fn new(secret: impl AsRef<[u8]>) -> Result<Self> {
        let inner =
            <Hmac<Sha512> as Mac>::new_from_slice(secret.as_ref()).map_err(|_| Error::InvalidSecretLength)?;
        Ok(Self { inner })
    }

    /// Compute the HMAC-SHA512 signature of `payload`. Returns raw signature
    /// bytes wrapped in [`Signature`]; the caller chooses the encoding.
    pub fn sign(&self, payload: impl AsRef<[u8]>) -> Signature {
        let mut mac = self.inner.clone();
        Mac::update(&mut mac, payload.as_ref());
        let bytes = Mac::finalize(mac).into_bytes().to_vec();
        Signature::from_bytes(bytes)
    }

    /// Constant-time verify `sig` against the HMAC of `payload`.
    pub fn verify(&self, sig: &Signature, payload: impl AsRef<[u8]>) -> Result<()> {
        let mut mac = self.inner.clone();
        Mac::update(&mut mac, payload.as_ref());
        Mac::verify_slice(mac, sig.as_bytes()).map_err(|_| Error::SymmetricVerifyFailed)
    }
}
