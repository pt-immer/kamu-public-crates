//! SNAP BI RSA-SHA256 signing and verification.
//!
//! The crate exposes only the protocol-mandated PKCS#1 v1.5 + SHA-256 scheme.

use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::traits::PublicKeyParts;

use crate::error::{Error, Result};
use crate::signature::Signature;

/// PKCS#1 v1.5 + SHA-256 signer for SNAP BI OAuth requests.
#[derive(Clone)]
pub struct RsaSigner {
    inner: rsa::pkcs1v15::SigningKey<sha2::Sha256>,
}

impl RsaSigner {
    /// Parse a PKCS#8-encoded private key PEM.
    ///
    /// Legacy PKCS#1 PEMs are rejected by upstream — convert to PKCS#8 with
    /// `openssl pkcs8 -topk8 -nocrypt -in pkcs1.pem -out pkcs8.pem` first.
    pub fn from_pkcs8_pem(pem: &str) -> Result<Self> {
        let inner = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::from_pkcs8_pem(pem)
            .map_err(|error| Error::InvalidSecretKey(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Sign `payload`.
    pub fn sign(&self, payload: impl AsRef<[u8]>) -> Signature {
        let signature = Signer::sign(&self.inner, payload.as_ref());
        Signature::from_bytes(signature.to_bytes().into_vec())
    }
}

/// PKCS#1 v1.5 + SHA-256 verifier for SNAP BI OAuth requests.
#[derive(Clone)]
pub struct RsaVerifier {
    inner: rsa::pkcs1v15::VerifyingKey<sha2::Sha256>,
}

impl RsaVerifier {
    /// Parse a SubjectPublicKeyInfo (SPKI) public-key PEM.
    pub fn from_spki_pem(pem: &str) -> Result<Self> {
        let inner = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::from_public_key_pem(pem)
            .map_err(|error| Error::InvalidPublicKey(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Verify `sig` against `payload`.
    pub fn verify(&self, sig: &Signature, payload: impl AsRef<[u8]>) -> Result<()> {
        if sig.as_bytes().len() != self.inner.as_ref().size() {
            return Err(Error::InvalidRawSignature(format!(
                "expected {} bytes, got {}",
                self.inner.as_ref().size(),
                sig.as_bytes().len(),
            )));
        }
        let signature = rsa::pkcs1v15::Signature::try_from(sig.as_bytes())
            .map_err(|error| Error::InvalidRawSignature(error.to_string()))?;
        Verifier::verify(&self.inner, payload.as_ref(), &signature).map_err(|_| Error::AsymmetricVerifyFailed)
    }
}
