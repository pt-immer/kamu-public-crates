//! Exact ISO 4217 money with compile-time currency identity.
//!
//! [`Money<USD>`](Money) and [`Money<IDR>`](Money) are different types, so
//! cross-currency arithmetic is a compile error. Addition and subtraction are
//! exact. Division returns a [`Division`] and requires a named residue decision
//! before releasing its quotient.
//!
//! # Start here
//!
//! ```
//! use kamu_money_core::{Money, iso::USD};
//!
//! let whole = Money::<USD>::try_from_major(10)?;
//! let parts = whole.allocate(&[1, 1, 1])?;
//! assert_eq!(Money::<USD>::try_sum(parts)?, whole);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The crate root holds the common path. Browse deeper only when needed:
//!
//! - [`allocation`] names lazy split results;
//! - [`locale`] and [`text`] own display and canonical text;
//! - [`errors`] groups narrow operation errors;
//! - [`advanced`] exposes raw-unit kernels, domain constants, residue internals,
//!   and stable hashing;
//! - feature-gated `wire` and [`adapters`] expose boundary integrations.
// Enables per-item "Available on crate feature ..." banners on docs.rs. `cfg_attr` keeps the
// nightly-only feature out of stable builds.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code, missing_docs)]
#![deny(clippy::all, clippy::pedantic, clippy::cargo)]
// This workspace-wide lint flags published feature names owned by other crates.
#![allow(clippy::redundant_feature_names)]
// Select strict lints individually; restriction and nursery are not stable aggregate policies.
#![deny(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_const_for_fn,
    clippy::use_self
)]
mod allocation;
mod arithmetic;
mod currency;
mod domain;
mod macros;
mod money;
mod rate;
mod residue;
mod rounding;
mod sealed {
    pub trait Sealed {}
}
mod stable_hash;

pub mod adapters;
pub mod advanced;
pub mod errors;
pub mod iso;
pub mod locale;
pub mod text;
#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
pub mod wire;

pub use allocation::SplitParts;
pub use currency::StaticCurrency;
pub use errors::MoneyError;
pub use iso::Iso4217;
pub use money::Money;
pub use rate::Rate;
pub use residue::{Division, Residue};
pub use rounding::Rounding;
