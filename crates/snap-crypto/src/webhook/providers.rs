//! Built-in [`WebhookVerifier`] implementations for known PT IMMER providers.
//!
//! All canonical-payload shapes here reflect the *current* upstream contract
//! at the time of writing; if a provider changes the recipe, override the
//! shipped impl or implement [`WebhookVerifier`] directly for the new shape.

use http::HeaderMap;

use crate::error::Result;
use crate::signature::Signature;

use super::{WebhookVerifier, header_str, hmac_compare};

/// Inacash cashout-callback verifier. Canonical payload = body bytes; signature
/// arrives in the `X-Signature` header as base64-encoded HMAC-SHA512.
pub struct InacashCashoutVerifier {
    /// Shared secret issued by Inacash for this merchant.
    pub secret: String,
}

impl WebhookVerifier for InacashCashoutVerifier {
    fn canonical_payload(&self, _headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>> {
        Ok(body.to_vec())
    }

    fn extract_signature(&self, headers: &HeaderMap) -> Result<Signature> {
        Signature::from_base64(header_str(headers, "X-Signature")?)
    }

    fn compare(&self, sig: &Signature, canonical: &[u8]) -> Result<()> {
        hmac_compare(self.secret.as_bytes(), sig, canonical)
    }
}

/// Inacash QRIS-status verifier. Same canonical shape as cashout, separate
/// secret slot so rotation is independent.
pub struct InacashQrisVerifier {
    /// Shared secret issued by Inacash for the QRIS product.
    pub secret: String,
}

impl WebhookVerifier for InacashQrisVerifier {
    fn canonical_payload(&self, _headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>> {
        Ok(body.to_vec())
    }

    fn extract_signature(&self, headers: &HeaderMap) -> Result<Signature> {
        Signature::from_base64(header_str(headers, "X-Signature")?)
    }

    fn compare(&self, sig: &Signature, canonical: &[u8]) -> Result<()> {
        hmac_compare(self.secret.as_bytes(), sig, canonical)
    }
}

/// BRI VA paid-status callback verifier. Canonical payload follows the SNAP BI
/// service `stringToSign` recipe; signature arrives in `X-SIGNATURE` as base64.
pub struct BriVaPaidVerifier {
    /// Client secret issued by BRI for this product.
    pub secret: String,
}

impl WebhookVerifier for BriVaPaidVerifier {
    fn canonical_payload(&self, _headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>> {
        // Real implementation should reconstruct the full SNAP BI stringToSign
        // including HTTP method, path, timestamp, and access-token from request
        // metadata. For the v2.0 cut this returns the raw body — adapter
        // subcrates (`kamu-snap-crypto-actix` / `-axum`) plug in the missing
        // request-context fields by overriding this method.
        Ok(body.to_vec())
    }

    fn extract_signature(&self, headers: &HeaderMap) -> Result<Signature> {
        Signature::from_base64(header_str(headers, "X-SIGNATURE")?)
    }

    fn compare(&self, sig: &Signature, canonical: &[u8]) -> Result<()> {
        hmac_compare(self.secret.as_bytes(), sig, canonical)
    }
}
