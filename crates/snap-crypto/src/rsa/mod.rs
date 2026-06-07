//! RSA signing and verification, generic over [`SignatureScheme`].
//!
//! [`RsaSigner`] and [`RsaVerifier`] are both generic over the chosen scheme.
//! Four built-in schemes are shipped:
//!
//! | Scheme           | Padding         | Hash       | SNAP BI default |
//! |------------------|-----------------|------------|-----------------|
//! | [`Pkcs1v15Sha256`] | PKCS#1-v1.5     | SHA-256    | ✓              |
//! | [`Pkcs1v15Sha512`] | PKCS#1-v1.5     | SHA-512    |                |
//! | [`PssSha256`]      | PSS (random salt) | SHA-256  |                |
//! | [`PssSha512`]      | PSS (random salt) | SHA-512  |                |
//!
//! The [`SignatureScheme`] trait is *sealed* — third-party crates cannot add
//! schemes — so the algorithm surface stays auditable. Future SNAP BI variants
//! that mandate new schemes will extend this enum upstream.
//!
//! All signers and verifiers take `&self` and clone the underlying key as
//! needed, allowing a single instance to be shared across threads without
//! external locking.

mod scheme;

pub use scheme::{Pkcs1v15Sha256, Pkcs1v15Sha512, PssSha256, PssSha512, SignatureScheme};

use crate::error::Result;
use crate::signature::Signature;

/// RSA signer parameterised by [`SignatureScheme`] (defaults to
/// [`Pkcs1v15Sha256`], the SNAP BI default).
#[derive(Clone)]
pub struct RsaSigner<S: SignatureScheme = Pkcs1v15Sha256> {
    inner: S::SigningKey,
}

impl<S: SignatureScheme> RsaSigner<S> {
    /// Parse a PKCS#8-encoded private key PEM.
    ///
    /// Legacy PKCS#1 PEMs are rejected by upstream — convert to PKCS#8 with
    /// `openssl pkcs8 -topk8 -nocrypt -in pkcs1.pem -out pkcs8.pem` first.
    pub fn from_pkcs8_pem(pem: &str) -> Result<Self> {
        let inner = S::parse_private_pem(pem)?;
        Ok(Self { inner })
    }

    /// Sign `payload` using the configured scheme. PSS schemes use
    /// `rand_core::OsRng` for the salt; PKCS#1-v1.5 is deterministic.
    pub fn sign(&self, payload: impl AsRef<[u8]>) -> Signature {
        S::sign(&self.inner, payload.as_ref())
    }
}

/// RSA verifier parameterised by [`SignatureScheme`] (defaults to
/// [`Pkcs1v15Sha256`]).
#[derive(Clone)]
pub struct RsaVerifier<S: SignatureScheme = Pkcs1v15Sha256> {
    inner: S::VerifyingKey,
}

impl<S: SignatureScheme> RsaVerifier<S> {
    /// Parse a PKCS#8 / SPKI-encoded public key PEM.
    pub fn from_pkcs8_public_pem(pem: &str) -> Result<Self> {
        let inner = S::parse_public_pem(pem)?;
        Ok(Self { inner })
    }

    /// Verify `sig` against `payload` using the configured scheme.
    pub fn verify(&self, sig: &Signature, payload: impl AsRef<[u8]>) -> Result<()> {
        S::verify(&self.inner, payload.as_ref(), sig.as_bytes())
    }
}
