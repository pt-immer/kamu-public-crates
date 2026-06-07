//! Framework-agnostic cryptography for Bank Indonesia SNAP BI integrations.
//!
//! This crate is the cryptographic spine shared by every PT IMMER SNAP BI
//! consumer (client and server). It exposes:
//!
//! - [`HmacSigner`] — HMAC-SHA512 sign/verify with constant-time verification.
//! - [`RsaSigner`] / [`RsaVerifier`] — PKCS#8-only RSA, generic over
//!   [`SignatureScheme`] (PKCS#1-v1.5 and PSS, SHA-256 and SHA-512).
//! - [`Signature`] + [`Encoding`] — encoding-agnostic signature bytes; callers
//!   pick base64 / base64url-nopad / lowercase-hex at the call site.
//! - [`snap_bi`] (feature `snap-bi`, default on) — canonical stringToSign
//!   builders, SHA-256/512 lower-hex helpers, Jakarta timestamp formatters, and
//!   SNAP BI header builders.
//! - [`webhook`] (feature `webhook`, default on) — provider-extensible webhook
//!   signature verification (Inacash, BRI VA paid, etc.).
//!
//! # Security guarantees
//!
//! - **No `unsafe`** anywhere in this crate (`#![forbid(unsafe_code)]`).
//! - **PKCS#8 enforcement**: legacy PKCS#1 PEMs are rejected by the upstream
//!   `rsa` crate parsers used here.
//! - **Constant-time verification**: HMAC verification uses
//!   `hmac::Mac::verify_slice`; RSA verification delegates to
//!   `rsa::signature::Verifier::verify` which is constant-time per upstream
//!   documentation.
//! - **No `actix-web` or HTTP-framework coupling**: this is a leaf crate.
//!   `wasm32-unknown-unknown` compiles require the downstream consumer to
//!   enable `getrandom/js` (pulled transitively via `rsa`).

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
pub use rsa::{RsaSigner, RsaVerifier, SignatureScheme};
pub use signature::{Encoding, Signature};
