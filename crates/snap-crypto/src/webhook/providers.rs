//! Built-in provider verifiers.

use core::fmt;

use http::HeaderMap;

#[cfg(feature = "snap-bi")]
use crate::snap_bi::{ServiceRequestParts, ServiceVerificationError, request::verify_service_request_with};
use crate::{HmacSigner, Result, Signature};

use super::BodyWebhookVerifier;
#[cfg(feature = "snap-bi")]
use super::RequestWebhookVerifier;

/// Body-only HMAC-SHA512 verifier.
///
/// Signatures use standard padded base64 in `X-Signature`.
#[derive(Clone)]
pub struct BodyHmacVerifier {
    signer: HmacSigner,
}

impl BodyHmacVerifier {
    /// Initialise with the provider secret.
    #[must_use]
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        Self { signer: HmacSigner::new(secret) }
    }
}

impl BodyWebhookVerifier for BodyHmacVerifier {
    fn verify_body(&self, headers: &HeaderMap, body: &[u8]) -> Result<()> {
        let signature = headers
            .get("x-signature")
            .ok_or(crate::Error::MissingHeader { name: "X-Signature" })?
            .to_str()
            .map_err(|_| crate::Error::InvalidHeader { name: "X-Signature" })?;
        self.signer.verify(&Signature::from_base64(signature)?, body)
    }
}

impl fmt::Debug for BodyHmacVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("BodyHmacVerifier").field("signer", &"[REDACTED]").finish()
    }
}

/// Inacash cashout callback verifier.
pub type InacashCashoutVerifier = BodyHmacVerifier;

/// Inacash QRIS status callback verifier.
pub type InacashQrisVerifier = BodyHmacVerifier;

/// Request-aware BRI VA paid-status verifier.
#[cfg(feature = "snap-bi")]
#[derive(Clone)]
pub struct BriVaPaidVerifier {
    signer: HmacSigner,
}

#[cfg(feature = "snap-bi")]
impl BriVaPaidVerifier {
    /// Initialise with the BRI client secret.
    #[must_use]
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        Self { signer: HmacSigner::new(secret) }
    }
}

#[cfg(feature = "snap-bi")]
impl RequestWebhookVerifier for BriVaPaidVerifier {
    fn verify_request(
        &self,
        request: ServiceRequestParts<'_>,
    ) -> core::result::Result<(), ServiceVerificationError> {
        verify_service_request_with(&self.signer, request)
    }
}

#[cfg(feature = "snap-bi")]
impl fmt::Debug for BriVaPaidVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("BriVaPaidVerifier").field("signer", &"[REDACTED]").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "snap-bi")]
    use crate::snap_bi::{AccessToken, ServiceStringToSign, sign_service};
    use crate::webhook::BodyWebhookVerifier;
    #[cfg(feature = "snap-bi")]
    use crate::webhook::RequestWebhookVerifier;

    fn signed_headers(secret: &str, body: &[u8]) -> HeaderMap {
        let signature = HmacSigner::new(secret).sign(body).to_base64();
        let mut headers = HeaderMap::new();
        headers.insert("x-signature", http::HeaderValue::from_str(&signature).unwrap());
        headers
    }

    #[test]
    fn body_hmac_round_trip() {
        let body = br#"{"status":"PAID"}"#;
        let verifier = InacashCashoutVerifier::new("cashout-secret");
        assert!(verifier.verify_body(&signed_headers("cashout-secret", body), body).is_ok());
    }

    #[test]
    fn body_hmac_rejects_tampering() {
        let verifier = InacashQrisVerifier::new("qris-secret");
        let headers = signed_headers("qris-secret", b"original");
        assert!(verifier.verify_body(&headers, b"tampered").is_err());
    }

    #[test]
    fn body_hmac_requires_signature_header() {
        let verifier = BodyHmacVerifier::new("secret");
        assert!(matches!(
            verifier.verify_body(&HeaderMap::new(), b"body"),
            Err(crate::Error::MissingHeader { .. })
        ));
    }

    #[cfg(feature = "snap-bi")]
    #[test]
    fn bri_va_verifies_full_request_context() {
        let method = http::Method::POST;
        let path = "/snap/v1.0/transfer-va/payment";
        let authorization = "Bearer access-token";
        let timestamp = "2026-07-28T12:34:56+07:00";
        let body = br#"{"status":"PAID"}"#;
        let token = AccessToken::parse(authorization).unwrap();
        let canonical = ServiceStringToSign::new(&method, path, token, body, timestamp).unwrap();
        let signature = sign_service("bri-secret", &canonical).to_base64();
        let request =
            ServiceRequestParts::new(&method, path, authorization, &signature, timestamp, body).unwrap();

        let verifier = BriVaPaidVerifier::new("bri-secret");
        assert!(verifier.verify_request(request).is_ok());
    }

    #[test]
    fn body_verifier_debug_redacts_secret() {
        let body_debug = format!("{:?}", BodyHmacVerifier::new("body-secret"));
        assert!(!body_debug.contains("body-secret"));
    }

    #[cfg(feature = "snap-bi")]
    #[test]
    fn request_verifier_debug_redacts_secret() {
        let request_debug = format!("{:?}", BriVaPaidVerifier::new("request-secret"));
        assert!(!request_debug.contains("request-secret"));
    }
}
