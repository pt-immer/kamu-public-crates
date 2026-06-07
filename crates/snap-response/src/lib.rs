//! Framework-agnostic response envelope + error taxonomy for SNAP BI.
//!
//! This crate is the response half of the SNAP BI spine (the crypto half lives
//! in `kamu-snap-crypto`). It exposes:
//!
//! - [`SnapResponse<T>`] — typed envelope + optional typed payload with
//!   spec-compliant flat JSON wire shape.
//! - [`SnapEnvelope`] — the envelope fields (`responseCode` +
//!   `responseMessage`).
//! - [`Error`] — the 61-variant SNAP BI error taxonomy plus a feature-gated
//!   `Crypto` bridge (feature `crypto`).
//! - [`Category`] — coarse retry/policy classification.
//! - [`ResponseCode`] — wire newtype with defensive parsing + an inverse
//!   classifier that maps received codes back to [`Error`] variants.
//! - [`ServiceCode`] — validated 0..=99 newtype.
//!
//! # Framework support
//!
//! This crate is framework-free. The `Responder` (actix-web) and
//! `IntoResponse` (axum) impls live in the per-framework adapter crates
//! (`kamu-snap-response-actix`, `kamu-snap-response-axum`).
//!
//! # Feature flags
//!
//! - `crypto` (default off) — enables the `Error::Crypto` variant which wraps a
//!   `kamu_snap_crypto::Error` for server-side handlers that propagate crypto
//!   failures into the SNAP error taxonomy.

#![forbid(unsafe_code)]

pub mod category;
pub mod envelope;
pub mod error;
pub mod response_code;

pub use category::Category;
pub use envelope::{SnapEnvelope, SnapResponse};
pub use error::Error;
pub use response_code::{ResponseCode, ServiceCode};
