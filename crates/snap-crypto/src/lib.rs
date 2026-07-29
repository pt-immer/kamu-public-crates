//! Framework-neutral authentication for Bank Indonesia SNAP BI.
//!
//! Common request paths are available at the crate root:
//!
//! - [`ServiceRequest`] validates and signs outbound service requests.
//! - [`ServiceRequestParts`] and [`verify_service_request`] validate inbound
//!   requests.
//! - [`HmacSigner`] provides HMAC-SHA512.
//! - [`RsaSigner`] and [`RsaVerifier`] provide PKCS#1 v1.5 + SHA-256 using
//!   PKCS#8 private keys and SPKI public keys.
//! - [`Signature`] provides standard base64, unpadded base64url, and lowercase
//!   hexadecimal encodings.
//!
//! Advanced recipes live in [`snap_bi`]; provider contracts live in
//! [`webhook`].
//!
//! # Security boundaries
//!
//! This crate forbids unsafe code. HMAC verification uses
//! `hmac::Mac::verify_slice`. The RSA dependency remains subject to
//! [RUSTSEC-2023-0071]; see the repository policy for its accepted scope.
//!
//! [RUSTSEC-2023-0071]: https://rustsec.org/advisories/RUSTSEC-2023-0071.html

#![forbid(unsafe_code)]

pub mod error;
pub mod hmac;
pub mod rsa;
pub mod signature;

#[cfg(feature = "snap-bi")]
pub mod snap_bi;

#[cfg(feature = "webhook")]
pub mod webhook;

pub use error::{Error, Result};
pub use hmac::HmacSigner;
pub use rsa::{RsaSigner, RsaVerifier};
pub use signature::{Encoding, Signature};

#[cfg(feature = "snap-bi")]
pub use snap_bi::{
    AccessToken, ServiceRequest, ServiceRequestParts, ServiceVerificationError, Signed, Unsigned,
    verify_service_request,
};
