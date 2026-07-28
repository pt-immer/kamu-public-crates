//! Money as an exact quantity: `i128` at fixed scale 18.
//!
//! The currency lives in the type and only in the type, so `Money<USD> + Money<IDR>` is a
//! compile error rather than a runtime one, and a `Money<USD>` is exactly 16 bytes.
//!
//! `+` and `-` cannot round. Division can, and it returns a [`Division`] that will not give up
//! its quotient until you say what happens to the [`Residue`]. See `DESIGN.md` for the
//! measurements that produced this design — in particular why `rust_decimal` is absent.
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

pub mod allocate;
pub mod arith;
pub mod currency;
pub mod domain;
pub mod error;
pub mod iso;
pub mod locale;
pub mod money;
#[cfg(feature = "postgres")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgres")))]
pub mod pg;
pub mod rate;
pub mod residue;
pub mod rounding;
#[cfg(feature = "sqlx")]
#[cfg_attr(docsrs, doc(cfg(feature = "sqlx")))]
pub mod sqlx_pg;
pub mod stable_hash;
pub mod text;
#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
pub mod wire;

pub use currency::StaticCurrency;
pub use domain::{DOMAIN_MAX, POW10_SCALE, SCALE};
pub use error::{
    AllocationError, AmountError, LocaleError, MoneyError, ParseMoneyError, RateError, WireError,
};
pub use iso::Iso4217;
pub use money::Money;
pub use rate::Rate;
pub use residue::{Division, Residue, UntaggedDivision};
pub use rounding::Rounding;
