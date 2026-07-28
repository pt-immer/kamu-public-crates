//! Provider-specific webhook verification.
//!
//! Body-only and full-request recipes use separate traits, so a provider that
//! signs request metadata cannot be represented as a body verifier.

pub mod providers;

#[cfg(feature = "snap-bi")]
pub use providers::BriVaPaidVerifier;
pub use providers::{BodyHmacVerifier, InacashCashoutVerifier, InacashQrisVerifier};

use http::HeaderMap;

use crate::Result;
#[cfg(feature = "snap-bi")]
use crate::snap_bi::{ServiceRequestParts, ServiceVerificationError};

/// Verifier for providers whose canonical payload is exactly the body bytes.
pub trait BodyWebhookVerifier {
    /// Verify the provider signature carried in `headers`.
    fn verify_body(&self, headers: &HeaderMap, body: &[u8]) -> Result<()>;
}

/// Verifier for providers whose signature covers full HTTP request context.
#[cfg(feature = "snap-bi")]
pub trait RequestWebhookVerifier {
    /// Verify a validated SNAP BI request.
    fn verify_request(
        &self,
        request: ServiceRequestParts<'_>,
    ) -> core::result::Result<(), ServiceVerificationError>;
}
