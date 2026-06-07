//! Sealed [`SignatureScheme`] trait + the four built-in schemes.

use rsa::signature::{RandomizedSigner, SignatureEncoding, Signer, Verifier};

use crate::error::{Error, Result};
use crate::signature::{Encoding, Signature};

mod sealed {
    /// Sealing marker — only this crate can implement
    /// [`super::SignatureScheme`].
    pub trait Sealed {}
}

/// Sealed trait describing one RSA signature scheme.
///
/// Implementations carry the upstream key + signature types as associated
/// types. Adding a new scheme is intentionally an in-crate change so the
/// algorithm matrix stays small and auditable.
pub trait SignatureScheme: sealed::Sealed {
    /// Upstream signing-key type.
    type SigningKey: Clone;

    /// Upstream verifying-key type.
    type VerifyingKey: Clone;

    /// Parse a PKCS#8 PEM into a signing key.
    fn parse_private_pem(pem: &str) -> Result<Self::SigningKey>;

    /// Parse a SPKI PEM into a verifying key.
    fn parse_public_pem(pem: &str) -> Result<Self::VerifyingKey>;

    /// Sign `payload` with `key`. Schemes that require randomness (PSS)
    /// source it from `rand_core::OsRng`.
    fn sign(key: &Self::SigningKey, payload: &[u8]) -> Signature;

    /// Verify the given raw signature bytes against `payload`.
    fn verify(key: &Self::VerifyingKey, payload: &[u8], sig_bytes: &[u8]) -> Result<()>;
}

fn map_priv<E: core::fmt::Display>(e: E) -> Error {
    Error::InvalidSecretKey(e.to_string())
}

fn map_pub<E: core::fmt::Display>(e: E) -> Error {
    Error::InvalidPublicKey(e.to_string())
}

fn map_sig_decode<E: core::fmt::Display>(e: E) -> Error {
    Error::SignatureDecode { encoding: Encoding::Base64, reason: e.to_string() }
}

// -- PKCS#1 v1.5 + SHA-256 --------------------------------------------------

/// PKCS#1 v1.5 padding over SHA-256. SNAP BI's mandated default.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pkcs1v15Sha256;

impl sealed::Sealed for Pkcs1v15Sha256 {}

impl SignatureScheme for Pkcs1v15Sha256 {
    type SigningKey = rsa::pkcs1v15::SigningKey<sha2::Sha256>;
    type VerifyingKey = rsa::pkcs1v15::VerifyingKey<sha2::Sha256>;

    fn parse_private_pem(pem: &str) -> Result<Self::SigningKey> {
        use rsa::pkcs8::DecodePrivateKey;
        Self::SigningKey::from_pkcs8_pem(pem).map_err(map_priv)
    }

    fn parse_public_pem(pem: &str) -> Result<Self::VerifyingKey> {
        use rsa::pkcs8::DecodePublicKey;
        Self::VerifyingKey::from_public_key_pem(pem).map_err(map_pub)
    }

    fn sign(key: &Self::SigningKey, payload: &[u8]) -> Signature {
        let sig = Signer::sign(key, payload);
        Signature::from_bytes(sig.to_bytes().into_vec())
    }

    fn verify(key: &Self::VerifyingKey, payload: &[u8], sig_bytes: &[u8]) -> Result<()> {
        let sig = rsa::pkcs1v15::Signature::try_from(sig_bytes).map_err(map_sig_decode)?;
        Verifier::verify(key, payload, &sig).map_err(|_| Error::AsymmetricVerifyFailed)
    }
}

// -- PKCS#1 v1.5 + SHA-512 --------------------------------------------------

/// PKCS#1 v1.5 padding over SHA-512.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pkcs1v15Sha512;

impl sealed::Sealed for Pkcs1v15Sha512 {}

impl SignatureScheme for Pkcs1v15Sha512 {
    type SigningKey = rsa::pkcs1v15::SigningKey<sha2::Sha512>;
    type VerifyingKey = rsa::pkcs1v15::VerifyingKey<sha2::Sha512>;

    fn parse_private_pem(pem: &str) -> Result<Self::SigningKey> {
        use rsa::pkcs8::DecodePrivateKey;
        Self::SigningKey::from_pkcs8_pem(pem).map_err(map_priv)
    }

    fn parse_public_pem(pem: &str) -> Result<Self::VerifyingKey> {
        use rsa::pkcs8::DecodePublicKey;
        Self::VerifyingKey::from_public_key_pem(pem).map_err(map_pub)
    }

    fn sign(key: &Self::SigningKey, payload: &[u8]) -> Signature {
        let sig = Signer::sign(key, payload);
        Signature::from_bytes(sig.to_bytes().into_vec())
    }

    fn verify(key: &Self::VerifyingKey, payload: &[u8], sig_bytes: &[u8]) -> Result<()> {
        let sig = rsa::pkcs1v15::Signature::try_from(sig_bytes).map_err(map_sig_decode)?;
        Verifier::verify(key, payload, &sig).map_err(|_| Error::AsymmetricVerifyFailed)
    }
}

// -- PSS + SHA-256 ----------------------------------------------------------

/// PSS padding over SHA-256. Randomised salt sourced from `OsRng`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PssSha256;

impl sealed::Sealed for PssSha256 {}

impl SignatureScheme for PssSha256 {
    type SigningKey = rsa::pss::SigningKey<sha2::Sha256>;
    type VerifyingKey = rsa::pss::VerifyingKey<sha2::Sha256>;

    fn parse_private_pem(pem: &str) -> Result<Self::SigningKey> {
        use rsa::pkcs8::DecodePrivateKey;
        let raw = rsa::RsaPrivateKey::from_pkcs8_pem(pem).map_err(map_priv)?;
        Ok(rsa::pss::SigningKey::<sha2::Sha256>::new(raw))
    }

    fn parse_public_pem(pem: &str) -> Result<Self::VerifyingKey> {
        use rsa::pkcs8::DecodePublicKey;
        let raw = rsa::RsaPublicKey::from_public_key_pem(pem).map_err(map_pub)?;
        Ok(rsa::pss::VerifyingKey::<sha2::Sha256>::new(raw))
    }

    fn sign(key: &Self::SigningKey, payload: &[u8]) -> Signature {
        let mut rng = rand_core::OsRng;
        let sig = RandomizedSigner::sign_with_rng(key, &mut rng, payload);
        Signature::from_bytes(sig.to_bytes().into_vec())
    }

    fn verify(key: &Self::VerifyingKey, payload: &[u8], sig_bytes: &[u8]) -> Result<()> {
        let sig = rsa::pss::Signature::try_from(sig_bytes).map_err(map_sig_decode)?;
        Verifier::verify(key, payload, &sig).map_err(|_| Error::AsymmetricVerifyFailed)
    }
}

// -- PSS + SHA-512 ----------------------------------------------------------

/// PSS padding over SHA-512. Randomised salt sourced from `OsRng`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PssSha512;

impl sealed::Sealed for PssSha512 {}

impl SignatureScheme for PssSha512 {
    type SigningKey = rsa::pss::SigningKey<sha2::Sha512>;
    type VerifyingKey = rsa::pss::VerifyingKey<sha2::Sha512>;

    fn parse_private_pem(pem: &str) -> Result<Self::SigningKey> {
        use rsa::pkcs8::DecodePrivateKey;
        let raw = rsa::RsaPrivateKey::from_pkcs8_pem(pem).map_err(map_priv)?;
        Ok(rsa::pss::SigningKey::<sha2::Sha512>::new(raw))
    }

    fn parse_public_pem(pem: &str) -> Result<Self::VerifyingKey> {
        use rsa::pkcs8::DecodePublicKey;
        let raw = rsa::RsaPublicKey::from_public_key_pem(pem).map_err(map_pub)?;
        Ok(rsa::pss::VerifyingKey::<sha2::Sha512>::new(raw))
    }

    fn sign(key: &Self::SigningKey, payload: &[u8]) -> Signature {
        let mut rng = rand_core::OsRng;
        let sig = RandomizedSigner::sign_with_rng(key, &mut rng, payload);
        Signature::from_bytes(sig.to_bytes().into_vec())
    }

    fn verify(key: &Self::VerifyingKey, payload: &[u8], sig_bytes: &[u8]) -> Result<()> {
        let sig = rsa::pss::Signature::try_from(sig_bytes).map_err(map_sig_decode)?;
        Verifier::verify(key, payload, &sig).map_err(|_| Error::AsymmetricVerifyFailed)
    }
}
