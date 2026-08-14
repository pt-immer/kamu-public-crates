//! Serde wire formats.
//!
//! Feature-gated behind `serde`, **default off** — turning it on only adds trait impls.
//!
//! # Two modes, selected per field
//!
//! | Mode | `Money<USD>` | `Rate<USD, IDR>` |
//! |---|---|---|
//! | [`structured`] (default) | `{"currency":"USD","amount":"10.50"}` | `{"base":"USD","quote":"IDR","rate":"16000"}` |
//! | [`transparent`] | `"USD 10.50"` | `"USD/IDR/16000"` |
//!
//! ```ignore
//! #[derive(Serialize, Deserialize)]
//! struct Payment {
//!     amount: Money<USD>,                                    // structured, the default
//!     #[serde(with = "kamu_money_core::wire::transparent")]
//!     fee: Money<USD>,                                       // "USD 10.50"
//! }
//! ```
//!
//! **Not a Cargo feature.** Features are additive and
//! unified across a dependency graph, so two crates wanting different formats would silently
//! get one, with no error. Per-field `#[serde(with = ...)]` gives the same compile-time
//! selection with no global coupling — and it *is* compile-time: a typo in the path is
//! `E0433: cannot find module`.
//!
//! # Binary is the same in both modes
//!
//! `is_human_readable() == false` emits `(ISO numeric, i128 units)` — the currency's numeric
//! code ahead of the units, `(base, quote, units)` for a `Rate`. The transparent/structured
//! split is a human-readable concern only; binary is one tagged shape either way.
//!
//! Binary data carries the ISO numeric tag because the reader chooses the Rust currency type.
//! A bare `i128` could otherwise cross-decode unchanged into a different denomination.
//!
//! # Types without codecs
//!
//! Error types, [`Rounding`](crate::Rounding), [`Residue`](crate::Residue), and
//! [`Division`](crate::Division). Errors and rounding policy are application
//! vocabulary. A residue is an accounting obligation tied to an operation, and
//! a division is unresolved local state; neither is a durable value to recreate
//! from untrusted input.

use crate::errors::{RateError, WireError};
use crate::iso::Iso4217;
use crate::text::{parse_amount, parse_rate_amount};
use crate::{Money, Rate, StaticCurrency};
use core::fmt;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod code;
mod impls;
pub mod structured;
pub mod transparent;

fn to_de_error<E: de::Error>(e: &impl fmt::Display) -> E {
    E::custom(format_args!("{e}"))
}

pub(super) fn money_from_units<C: StaticCurrency, E: de::Error>(units: i128) -> Result<Money<C>, E> {
    Money::<C>::try_from_units(units).map_err(|error| to_de_error(&error))
}

// Unlike its `Money` twin above, this cannot synthesise the error: a `Rate` is refused for two
// different reasons (magnitude, sign) and the deserialiser is a feed ingress, so it forwards
// whichever one the constructor gave rather than assuming domain overflow.
pub(super) fn rate_from_units<Base: StaticCurrency, Quote: StaticCurrency, E: de::Error>(
    units: i128,
) -> Result<Rate<Base, Quote>, E> {
    Rate::try_from_units(units).map_err(|e| to_de_error(&e))
}

pub(super) fn money_from_amount<C: StaticCurrency>(amount: &str) -> Result<Money<C>, WireError> {
    let units = parse_amount(amount)?;
    Ok(Money::try_from_units(units)?)
}

pub(super) fn rate_from_amount<Base: StaticCurrency, Quote: StaticCurrency>(
    amount: &str,
) -> Result<Rate<Base, Quote>, WireError> {
    let units = parse_rate_amount(amount).map_err(RateError::from)?;
    Ok(Rate::try_from_units(units)?)
}

// Both serde modes share these tagged binary helpers. The standards-assigned ISO numeric code
// remains stable if the generated enum changes order.

pub(super) fn money_to_binary<C: StaticCurrency, S: Serializer>(
    m: Money<C>,
    s: S,
) -> Result<S::Ok, S::Error> {
    (C::CODE, m.units()).serialize(s)
}

pub(super) fn money_from_binary<'de, C: StaticCurrency, D: Deserializer<'de>>(
    d: D,
) -> Result<Money<C>, D::Error> {
    let (code, units) = <(Iso4217, i128)>::deserialize(d)?;
    if code != C::CODE {
        return Err(to_de_error(&WireError::WrongCurrency { expected: C::CODE, found: code }));
    }
    money_from_units(units)
}

pub(super) fn rate_to_binary<Base: StaticCurrency, Quote: StaticCurrency, S: Serializer>(
    r: Rate<Base, Quote>,
    s: S,
) -> Result<S::Ok, S::Error> {
    (Base::CODE, Quote::CODE, r.units()).serialize(s)
}

pub(super) fn rate_from_binary<'de, Base: StaticCurrency, Quote: StaticCurrency, D: Deserializer<'de>>(
    d: D,
) -> Result<Rate<Base, Quote>, D::Error> {
    let (base, quote, units) = <(Iso4217, Iso4217, i128)>::deserialize(d)?;
    // Both ends are checked, in declaration order, because a refactor moves a rate's pair far
    // more easily than its magnitude.
    if base != Base::CODE {
        return Err(to_de_error(&WireError::WrongCurrency { expected: Base::CODE, found: base }));
    }
    if quote != Quote::CODE {
        return Err(to_de_error(&WireError::WrongCurrency { expected: Quote::CODE, found: quote }));
    }
    rate_from_units(units)
}
