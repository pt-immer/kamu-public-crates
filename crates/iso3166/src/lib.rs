//! # `kamu-iso3166`
//!
//! Zero-allocation, `no_std`-compatible ISO 3166-1 and ISO 3166-2 primitives.
//!
//! ## Scope (v0.1)
//!
//! - ISO 3166-1: `Alpha2`, `Alpha3`, `Numeric` (see [`one`])
//! - ISO 3166-2: subdivisions keyed by parent country (see [`two`])
//!
//! ## Features
//!
//! - `std` (default) — enables `std::error::Error` integrations.
//! - `alloc` — reserved for future API surfaces that may accept owned strings.
//! - `serde` — derive `Serialize`/`Deserialize` for all public types.
//!
//! All lookups return `&'static` data; no runtime allocation is performed.
//!
//! ## Licensing
//!
//! Crate code is licensed under Apache-2.0. The embedded ISO 3166 data is
//! vendored from `ipregistry/iso3166` and is licensed under
//! Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0).
//! See `NOTICE` and `VENDORED.md` for full attribution.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod error;
pub mod one;
pub mod two;

#[cfg(feature = "serde")]
mod serde_impl;

pub use error::{ParseCountryError, ParseSubdivisionError};
pub use one::{Alpha2, Alpha3, Numeric};
pub use two::{Category, Subdivision};
