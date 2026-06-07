//! Webhook signature verification — provider-extensible.
//!
//! Every PT IMMER consumer integrates against multiple webhook sources
//! (Inacash cashout/QRIS, BRI VA paid-status, future BI-FAST credit
//! notifications). Each provider uses HMAC over a provider-specific
//! canonical payload with its own header conventions. The [`WebhookVerifier`]
//! trait expresses that pattern once; built-in implementations cover the
//! common providers, and consumers can ship their own.
//!
//! Feature-gated behind `webhook` (default on); pulls `http`.

pub mod providers;

pub use providers::{BriVaPaidVerifier, InacashCashoutVerifier, InacashQrisVerifier};

use http::HeaderMap;

use crate::error::{Error, Result};
use crate::signature::Signature;

/// Provider-specific webhook signature verification.
///
/// Default impls of [`WebhookVerifier::verify`] wire `canonical_payload` +
/// `extract_signature` + `compare` together. Implementers usually only need
/// to provide the first two; `compare` defaults to constant-time HMAC-SHA512
/// using a provider-supplied shared secret.
pub trait WebhookVerifier {
    /// Reconstruct the bytes the provider hashed when producing the inbound
    /// signature. Typically a concatenation of headers + body in a
    /// provider-prescribed format.
    fn canonical_payload(&self, headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>>;

    /// Extract the inbound signature bytes from the request headers, decoding
    /// from the provider's wire encoding (base64, hex, etc.).
    fn extract_signature(&self, headers: &HeaderMap) -> Result<Signature>;

    /// Constant-time compare `sig` against the canonical payload's expected
    /// signature. Default impl assumes HMAC-SHA512 with a shared secret;
    /// override for non-HMAC providers.
    fn compare(&self, sig: &Signature, canonical: &[u8]) -> Result<()>;

    /// Default end-to-end verification.
    fn verify(&self, headers: &HeaderMap, body: &[u8]) -> Result<()> {
        let canonical = self.canonical_payload(headers, body)?;
        let sig = self.extract_signature(headers)?;
        self.compare(&sig, &canonical)
    }
}

/// Helper used by built-in providers: constant-time HMAC-SHA512 compare with
/// a shared secret.
pub(crate) fn hmac_compare(secret: &[u8], sig: &Signature, canonical: &[u8]) -> Result<()> {
    let signer = crate::HmacSigner::new(secret)?;
    signer.verify(sig, canonical)
}

fn missing_header(name: &str) -> Error {
    Error::Webhook(format!("missing header: {name}"))
}

pub(crate) fn header_str<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str> {
    headers
        .get(name)
        .ok_or_else(|| missing_header(name))?
        .to_str()
        .map_err(|e| Error::Webhook(format!("non-ASCII header {name}: {e}")))
}
