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
// Enables the per-item "Available on crate feature ..." banners on docs.rs. Nightly-only,
// hence cfg_attr: a stable build simply does not see it.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code, missing_docs)]
#![deny(clippy::all, clippy::pedantic, clippy::cargo)]
// `clippy::cargo` lints the WORKSPACE's metadata, not just this crate's, so denying it here
// makes kamu-logging's published `with-otlp` / `with-actix-web` feature names errors in this
// crate's build. They are that crate's public API, renaming them is a breaking change, and
// this crate cannot fix them — so the one cargo lint that reaches across is allowed and the
// rest of the group stays denied.
#![allow(clippy::redundant_feature_names)]
// Cherry-picked from `clippy::restriction` and `clippy::nursery`, which are NOT meant to be
// enabled wholesale: `restriction` is self-contradictory by design (it exists to be sampled)
// and `nursery` is under development. Denying either group would let a toolchain upgrade break
// every local build for reasons unrelated to this code. Naming the lints gets the benefit
// without importing that instability. (DESIGN.md C10)
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
// The `cargo_common_metadata` allow that stood here is GONE. It existed because `repository` was
// missing from Cargo.toml while this repo had no remote — measured, not assumed. A remote exists
// now, the field is filled in from `git remote -v`, and the allow would otherwise have quietly
// outlived its reason and started covering the next piece of missing metadata instead.

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

// Compatibility paths for the pre-facade API. Hidden from the browsing surface
// but kept for one release so downstream migrations are compiler-guided.

/// Deprecated compatibility path; use [`allocation`] or [`advanced::arithmetic`].
#[deprecated(since = "0.1.0", note = "use `allocation` or `advanced::arithmetic`; removed in 0.2.0")]
#[doc(hidden)]
pub mod allocate {
    pub use crate::allocate_impl::*;
}

/// Deprecated compatibility path; use [`advanced::arithmetic`].
#[deprecated(since = "0.1.0", note = "use `advanced::arithmetic`; removed in 0.2.0")]
#[doc(hidden)]
pub mod arith {
    pub use crate::arith_impl::*;
}

/// Deprecated compatibility path; import [`StaticCurrency`] from the crate root.
#[deprecated(since = "0.1.0", note = "use root `StaticCurrency`; removed in 0.2.0")]
#[doc(hidden)]
pub mod currency {
    pub use crate::currency_impl::*;
}

/// Deprecated compatibility path; use [`advanced::domain`].
#[deprecated(since = "0.1.0", note = "use `advanced::domain`; removed in 0.2.0")]
#[doc(hidden)]
pub mod domain {
    pub use crate::domain_impl::*;
}

/// Deprecated compatibility path; use [`errors`].
#[deprecated(since = "0.1.0", note = "use `errors`; removed in 0.2.0")]
#[doc(hidden)]
pub mod error {
    pub use crate::error_impl::*;
}

/// Deprecated compatibility path; import [`Money`] from the crate root.
#[deprecated(since = "0.1.0", note = "use root `Money`; removed in 0.2.0")]
#[doc(hidden)]
pub mod money {
    pub use crate::money_impl::*;
}

/// Deprecated compatibility path; import [`Rate`] from the crate root.
#[deprecated(since = "0.1.0", note = "use root `Rate`; removed in 0.2.0")]
#[doc(hidden)]
pub mod rate {
    pub use crate::rate_impl::*;
}

/// Deprecated compatibility path; use root [`Division`] and [`Residue`] or
/// [`advanced::residue`].
#[deprecated(
    since = "0.1.0",
    note = "use root `Division`/`Residue` or `advanced::residue`; removed in 0.2.0"
)]
#[doc(hidden)]
pub mod residue {
    pub use crate::residue_impl::*;
}

/// Deprecated compatibility path; import [`Rounding`] from the crate root.
#[deprecated(since = "0.1.0", note = "use root `Rounding`; removed in 0.2.0")]
#[doc(hidden)]
pub mod rounding {
    pub use crate::rounding_impl::*;
}

/// Deprecated compatibility path; use [`advanced::stable_hash`].
#[deprecated(since = "0.1.0", note = "use `advanced::stable_hash`; removed in 0.2.0")]
#[doc(hidden)]
pub mod stable_hash {
    pub use crate::stable_hash_impl::*;
}

/// Deprecated compatibility path; use [`adapters::postgres`].
#[cfg(feature = "postgres")]
#[deprecated(since = "0.1.0", note = "use `adapters::postgres`; removed in 0.2.0")]
#[doc(hidden)]
pub mod pg {}

/// Deprecated compatibility path; use [`adapters::sqlx`].
#[cfg(feature = "sqlx")]
#[deprecated(since = "0.1.0", note = "use `adapters::sqlx`; removed in 0.2.0")]
#[doc(hidden)]
pub mod sqlx_pg {}

#[doc(hidden)]
#[deprecated(since = "0.1.0", note = "use `advanced::domain`; removed in 0.2.0")]
pub use domain_impl::{DOMAIN_MAX, POW10_SCALE, SCALE};
#[doc(hidden)]
#[deprecated(since = "0.1.0", note = "use `errors`; removed in 0.2.0")]
pub use error_impl::{AllocationError, AmountError, LocaleError, ParseMoneyError, RateError, WireError};
#[doc(hidden)]
#[deprecated(since = "0.1.0", note = "use `advanced::residue::UntaggedDivision`; removed in 0.2.0")]
pub use residue_impl::UntaggedDivision;
