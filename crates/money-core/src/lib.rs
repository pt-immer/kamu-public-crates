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
#[path = "allocate.rs"]
mod allocate_impl;
#[path = "arith.rs"]
mod arith_impl;
#[path = "currency.rs"]
mod currency_impl;
#[path = "domain.rs"]
mod domain_impl;
#[path = "error.rs"]
mod error_impl;
#[path = "macros.rs"]
mod macros_impl;
#[path = "money.rs"]
mod money_impl;
#[path = "rate.rs"]
mod rate_impl;
#[path = "residue.rs"]
mod residue_impl;
#[path = "rounding.rs"]
mod rounding_impl;
mod sealed {
    pub trait Sealed {}
}
#[path = "stable_hash.rs"]
mod stable_hash_impl;

pub mod adapters;
pub mod advanced;
pub mod allocation;
pub mod errors;
pub mod iso;
pub mod locale;
pub mod text;
#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
pub mod wire;

pub use currency_impl::StaticCurrency;
pub use error_impl::MoneyError;
pub use iso::Iso4217;
pub use money_impl::Money;
pub use rate_impl::Rate;
pub use residue_impl::{Division, Residue};
pub use rounding_impl::Rounding;
