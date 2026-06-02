//! # `kamu-iso3166`
//!
//! Zero-allocation, `no_std`-compatible ISO 3166-1 and ISO 3166-2 primitives.
//!
//! ## Scope
//!
//! - ISO 3166-1: [`Alpha2`], [`Alpha3`], [`Numeric`] (see [`country`])
//! - ISO 3166-2: subdivisions keyed by parent country (see [`subdivision`])
//! - ISO 3166-3: *out of scope*; planned for a later release.
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
//! Crate code is dual-licensed under `MIT OR Apache-2.0`. The embedded ISO 3166
//! data is vendored from `ipregistry/iso3166` and is licensed under Creative
//! Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0). See `NOTICE`
//! and `VENDORED.md` for full attribution.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod country;
pub mod error;
pub mod subdivision;

#[cfg(feature = "serde")]
mod serde_impl;

pub use country::{Alpha2, Alpha3, Numeric};
pub use error::{ParseCountryError, ParseSubdivisionError};
pub use subdivision::{Category, Subdivision};

/// Deprecated alias for [`country`] (ISO 3166-1), renamed in 0.2.0.
/// Use [`crate::country`] instead.
#[deprecated(since = "0.2.0", note = "module `one` was renamed to `country`")]
pub mod one {
    pub use crate::country::{Alpha2, Alpha3, Numeric};
}

/// Deprecated alias for [`subdivision`] (ISO 3166-2), renamed in 0.2.0.
/// Use [`crate::subdivision`] instead.
#[deprecated(since = "0.2.0", note = "module `two` was renamed to `subdivision`")]
pub mod two {
    pub use crate::subdivision::{Category, Subdivision};
}
