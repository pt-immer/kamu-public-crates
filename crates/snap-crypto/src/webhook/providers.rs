//! Built-in [`WebhookVerifier`] implementations for known PT IMMER providers.
//!
//! All canonical-payload shapes here reflect the *current* upstream contract
//! at the time of writing; if a provider changes the recipe, override the
//! shipped impl or implement [`WebhookVerifier`] directly for the new shape.

use http::HeaderMap;

use crate::error::{Error, Result};
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

/// BRI VA paid-status callback verifier.
///
/// **This base impl cannot verify a BRI VA callback on its own.** BRI VA signs
/// the full SNAP BI service `stringToSign`
/// (`method:path:accessToken:lowercaseHex(SHA256(body)):timestamp`), which
/// requires request context (HTTP method, path, access token, `X-TIMESTAMP`)
/// that a `(headers, body)` pair does not carry. So [`canonical_payload`] here
/// returns an error rather than silently HMAC-ing the raw body (which would
/// reject every legitimate callback, or pass only against a same-shaped test
/// signature and then fail in production).
///
/// To verify a BRI VA service signature, use the request-context-aware
/// `verify_request` in `kamu-snap-crypto-actix` / `kamu-snap-crypto-axum`, or
/// implement [`WebhookVerifier`] yourself with the request metadata in hand and
/// override [`canonical_payload`] to build the real `stringToSign`.
///
/// [`canonical_payload`]: WebhookVerifier::canonical_payload
pub struct BriVaPaidVerifier {
    /// Client secret issued by BRI for this product.
    pub secret: String,
}

impl WebhookVerifier for BriVaPaidVerifier {
    fn canonical_payload(&self, _headers: &HeaderMap, _body: &[u8]) -> Result<Vec<u8>> {
        Err(Error::Webhook(
            "BriVaPaidVerifier cannot verify from the body alone: BRI VA signs the SNAP BI \
             service stringToSign (method:path:accessToken:lowercaseHex(SHA256(body)):timestamp), \
             not the raw body. Use kamu-snap-crypto-actix / kamu-snap-crypto-axum `verify_request`, \
             or implement WebhookVerifier with the request context and override canonical_payload."
                .to_owned(),
        ))
    }

    fn extract_signature(&self, headers: &HeaderMap) -> Result<Signature> {
        Signature::from_base64(header_str(headers, "X-SIGNATURE")?)
    }

    fn compare(&self, sig: &Signature, canonical: &[u8]) -> Result<()> {
        hmac_compare(self.secret.as_bytes(), sig, canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HmacSigner;

    /// Build a header map carrying the base64 HMAC-SHA512 of `body` under `name`.
    fn signed_headers(name: &str, secret: &str, body: &[u8]) -> HeaderMap {
        let sig = HmacSigner::new(secret.as_bytes()).unwrap().sign(body).to_base64();
        let mut h = HeaderMap::new();
        let hn = http::HeaderName::from_bytes(name.as_bytes()).unwrap();
        h.insert(hn, http::HeaderValue::from_str(&sig).unwrap());
        h
    }

    #[test]
    fn inacash_cashout_round_trip() {
        let secret = "inacash-cashout-secret";
        let body = br#"{"status":"PAID"}"#;
        let v = InacashCashoutVerifier { secret: secret.to_owned() };
        let headers = signed_headers("X-Signature", secret, body);
        assert!(v.verify(&headers, body).is_ok());
    }

    #[test]
    fn inacash_cashout_rejects_tampered_body() {
        let secret = "inacash-cashout-secret";
        let v = InacashCashoutVerifier { secret: secret.to_owned() };
        let headers = signed_headers("X-Signature", secret, b"original-body");
        assert!(v.verify(&headers, b"tampered-body").is_err());
    }

    #[test]
    fn inacash_qris_round_trip_with_independent_secret() {
        let secret = "inacash-qris-secret";
        let body = br#"{"qris":"ok"}"#;
        let v = InacashQrisVerifier { secret: secret.to_owned() };
        let headers = signed_headers("X-Signature", secret, body);
        assert!(v.verify(&headers, body).is_ok());
    }

    #[test]
    fn missing_signature_header_errors() {
        let v = InacashCashoutVerifier { secret: "s".to_owned() };
        assert!(v.verify(&HeaderMap::new(), b"body").is_err());
    }

    #[test]
    fn bri_va_verifier_refuses_body_only_verification() {
        // Even with a body-HMAC that *would* match, BriVaPaidVerifier must fail
        // loud at canonical_payload — it cannot reconstruct the real stringToSign.
        let secret = "bri-secret";
        let v = BriVaPaidVerifier { secret: secret.to_owned() };
        let headers = signed_headers("X-SIGNATURE", secret, b"body");
        match v.verify(&headers, b"body") {
            Err(Error::Webhook(msg)) => assert!(msg.contains("stringToSign")),
            other => panic!("expected fail-loud Webhook error, got {other:?}"),
        }
    }
}
