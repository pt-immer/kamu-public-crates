//! High-level one-shot sign + verify functions over the SNAP BI recipes.
//!
//! These exist so callers don't have to learn the `HmacSigner` / `RsaSigner`
//! API just to do the most common operation. The lower-level types are still
//! available for advanced cases (e.g. reusing one signer across many requests).

use crate::error::Result;
use crate::hmac::HmacSigner;
use crate::rsa::{RsaSigner, RsaVerifier, SignatureScheme};
use crate::signature::Signature;

use super::string_to_sign::{OAuthStringToSign, ServiceStringToSign};

/// Sign a SNAP BI service request. Returns the raw signature; callers pick
/// the wire encoding (BRI prod accepts base64; some sandboxes accept hex).
pub fn sign_service(client_secret: impl AsRef<[u8]>, parts: &ServiceStringToSign<'_>) -> Result<Signature> {
    let signer = HmacSigner::new(client_secret)?;
    Ok(signer.sign(parts.build()))
}

/// Verify a received SNAP BI service signature.
pub fn verify_service(
    client_secret: impl AsRef<[u8]>,
    parts: &ServiceStringToSign<'_>,
    sig: &Signature,
) -> Result<()> {
    let signer = HmacSigner::new(client_secret)?;
    signer.verify(sig, parts.build())
}

/// Sign a SNAP BI OAuth `/access-token/b2b` request using the partner's
/// private RSA key.
pub fn sign_oauth<S: SignatureScheme>(signer: &RsaSigner<S>, parts: &OAuthStringToSign<'_>) -> Signature {
    signer.sign(parts.build())
}

/// Verify a SNAP BI OAuth signature using the partner's public RSA key.
pub fn verify_oauth<S: SignatureScheme>(
    verifier: &RsaVerifier<S>,
    parts: &OAuthStringToSign<'_>,
    sig: &Signature,
) -> Result<()> {
    verifier.verify(sig, parts.build())
}
