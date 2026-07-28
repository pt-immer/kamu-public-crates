//! The serde wire. (DESIGN.md C7)
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
//! **Not a Cargo feature.** C7 rejects feature-selected wire formats: features are additive and
//! unified across a dependency graph, so two crates wanting different formats would silently
//! get one, with no error. Per-field `#[serde(with = ...)]` gives the same compile-time
//! selection with no global coupling — and it *is* compile-time: a typo in the path is
//! `E0433: cannot find module`, measured, not a runtime surprise.
//!
//! # Binary is the same in both modes
//!
//! `is_human_readable() == false` emits `(ISO numeric, i128 units)` — the currency's numeric
//! code ahead of the units, `(base, quote, units)` for a `Rate`. The transparent/structured
//! split is a human-readable concern only; binary is one tagged shape either way.
//!
//! It did not always carry the tag. Through C7 the binary form was a bare `i128` — "the currency
//! is in the type, it costs zero bytes" — until an external review (R2-F2) showed that a bare
//! `i128` is exactly what lets `Money<USD>` bytes decode as `Money<IDR>`: the type is chosen by
//! the *reader*, independently of the writer, so serialization is the one boundary the
//! compile-time currency does not cross. The human-readable form always carried identity for
//! this reason; binary now does too. Two ISO-numeric bytes are the price of not silently
//! redenominating money, which is the one thing this crate exists to refuse.
//!
//! # What deliberately has NO codec
//!
//! Error types, [`Rounding`](crate::Rounding), [`Residue`](crate::Residue), and
//! [`Division`](crate::Division). Errors and rounding policy are application
//! vocabulary. A residue is an accounting obligation tied to an operation, and
//! a division is unresolved local state; neither is a durable value to recreate
//! from untrusted input.

use crate::Money;
use crate::Rate;
use crate::StaticCurrency;
use crate::error_impl::{RateError, WireError};
use crate::iso::Iso4217;
use crate::text::{parse_amount, parse_rate_amount, render_amount, render_rate};
use core::fmt;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

// ---------------------------------------------------------------------------------------
// Iso4217 — hand-written, and this is the one that MUST NOT be derived.
// ---------------------------------------------------------------------------------------

/// Hand-written on purpose. `#[derive(Serialize)]` encodes an enum variant by its **ordinal
/// position**, not its discriminant — measured: after inserting one currency mid-table, stored
/// `IDR` decoded as `GBP`, silently, with `#[repr(u16)]` and `IDR = 360` unchanged in both
/// versions.
///
/// That insert is still the plan, though the reason has changed and this comment used to give
/// the old one ("12 of ~180 and WILL grow"). The register is now complete at 178 codes, so it
/// no longer grows by being filled in — it grows because ISO issues codes. The variants are
/// emitted in alpha-3 order, so a new code is overwhelmingly likely to land *between* existing
/// ones rather than at the end, which is exactly the shift that moves every later ordinal.
/// Completeness removed the frequent case and left the dangerous one.
///
/// A JSON test suite **cannot** catch it: human-readable formats emit the variant *name* and
/// are unaffected. It surfaces only when old binary data meets a newer build.
///
/// `rename_all` is likewise forbidden: `SCREAMING_SNAKE_CASE` emits `"I_D_R"`, and it reads
/// *more* correct in the source than it behaves.
///
/// Both forms here are assigned by the standard — alpha-3 human, ISO **numeric** binary — so
/// neither is this crate's choice to make.
impl Serialize for Iso4217 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() { s.serialize_str(self.alpha3()) } else { s.serialize_u16(self.numeric()) }
    }
}

impl<'de> Deserialize<'de> for Iso4217 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct CodeVisitor;

        impl Visitor<'_> for CodeVisitor {
            type Value = Iso4217;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an ISO 4217 alpha-3 code or numeric-3 code")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Iso4217, E> {
                Iso4217::from_alpha3(v).ok_or_else(|| E::custom(format_args!("unknown ISO 4217 code {v:?}")))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Iso4217, E> {
                u16::try_from(v)
                    .ok()
                    .and_then(Iso4217::from_numeric)
                    .ok_or_else(|| E::custom(format_args!("unknown ISO 4217 numeric code {v}")))
            }
        }

        if d.is_human_readable() { d.deserialize_str(CodeVisitor) } else { d.deserialize_u16(CodeVisitor) }
    }
}

// ---------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------

fn to_de_error<E: de::Error>(e: &impl fmt::Display) -> E {
    E::custom(format_args!("{e}"))
}

fn money_from_units<C: StaticCurrency, E: de::Error>(units: i128) -> Result<Money<C>, E> {
    Money::<C>::try_from_units(units).map_err(|error| to_de_error(&error))
}

// Unlike its `Money` twin above, this cannot synthesise the error: a `Rate` is refused for two
// different reasons (magnitude, sign) and the deserialiser is a feed ingress, so it forwards
// whichever one the constructor gave rather than assuming domain overflow.
fn rate_from_units<Base: StaticCurrency, Quote: StaticCurrency, E: de::Error>(
    units: i128,
) -> Result<Rate<Base, Quote>, E> {
    Rate::try_from_units(units).map_err(|e| to_de_error(&e))
}

fn money_from_amount<C: StaticCurrency>(amount: &str) -> Result<Money<C>, WireError> {
    let units = parse_amount(amount)?;
    Ok(Money::try_from_units(units)?)
}

fn rate_from_amount<Base: StaticCurrency, Quote: StaticCurrency>(
    amount: &str,
) -> Result<Rate<Base, Quote>, WireError> {
    let units = parse_rate_amount(amount).map_err(RateError::from)?;
    Ok(Rate::try_from_units(units)?)
}

// --- binary codec (R2-F2) --------------------------------------------------------------
//
// The tag lives in ONE place, called by both the default impls and the transparent module, for
// the same reason the parser does: two encoders of the same value drift on exactly the inputs
// nobody tests. Binary carries the ISO numeric code because a bare `i128` has no identity — the
// R2-F2 defect was that `Money<USD>` bytes decoded as `Money<IDR>`, units preserved and currency
// silently reassigned. `(Iso4217, i128)` reuses the hand-written numeric codec, so the tag is the
// standards-assigned number, never the enum ordinal that shifts when a currency is inserted.

fn money_to_binary<C: StaticCurrency, S: Serializer>(m: Money<C>, s: S) -> Result<S::Ok, S::Error> {
    (C::CODE, m.units()).serialize(s)
}

fn money_from_binary<'de, C: StaticCurrency, D: Deserializer<'de>>(d: D) -> Result<Money<C>, D::Error> {
    let (code, units) = <(Iso4217, i128)>::deserialize(d)?;
    if code != C::CODE {
        return Err(to_de_error(&WireError::WrongCurrency { expected: C::CODE, found: code }));
    }
    money_from_units(units)
}

fn rate_to_binary<Base: StaticCurrency, Quote: StaticCurrency, S: Serializer>(
    r: Rate<Base, Quote>,
    s: S,
) -> Result<S::Ok, S::Error> {
    (Base::CODE, Quote::CODE, r.units()).serialize(s)
}

fn rate_from_binary<'de, Base: StaticCurrency, Quote: StaticCurrency, D: Deserializer<'de>>(
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

// ---------------------------------------------------------------------------------------
// Default impls: STRUCTURED
// ---------------------------------------------------------------------------------------

#[derive(Serialize)]
struct MoneyOut<'a> {
    currency: Iso4217,
    amount: &'a str,
}

#[derive(Deserialize)]
struct MoneyIn<'a> {
    currency: Iso4217,
    #[serde(borrow)]
    amount: Cow<'a, str>,
}

#[derive(Serialize)]
struct RateOut<'a> {
    base: Iso4217,
    quote: Iso4217,
    rate: &'a str,
}

#[derive(Deserialize)]
struct RateIn<'a> {
    base: Iso4217,
    quote: Iso4217,
    #[serde(borrow)]
    rate: Cow<'a, str>,
}

impl<C: StaticCurrency> Serialize for Money<C> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            MoneyOut { currency: C::CODE, amount: &render_amount(*self) }.serialize(s)
        } else {
            money_to_binary(*self, s)
        }
    }
}

impl<'de, C: StaticCurrency> Deserialize<'de> for Money<C> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if !d.is_human_readable() {
            return money_from_binary(d);
        }
        let raw = MoneyIn::deserialize(d)?;
        // The redundancy is the point: it catches an IDR amount in a USD field.
        if raw.currency != C::CODE {
            return Err(to_de_error(&WireError::WrongCurrency { expected: C::CODE, found: raw.currency }));
        }
        // Parse the amount field directly. Reconstructing a tagged string here
        // allocated and then made the text parser split a tag already checked
        // above.
        money_from_amount(raw.amount.as_ref()).map_err(|e| to_de_error(&e))
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Serialize for Rate<Base, Quote> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            RateOut { base: Base::CODE, quote: Quote::CODE, rate: &render_rate(*self) }.serialize(s)
        } else {
            rate_to_binary(*self, s)
        }
    }
}

impl<'de, Base: StaticCurrency, Quote: StaticCurrency> Deserialize<'de> for Rate<Base, Quote> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if !d.is_human_readable() {
            return rate_from_binary(d);
        }
        let raw = RateIn::deserialize(d)?;
        if raw.base != Base::CODE {
            return Err(to_de_error(&WireError::WrongCurrency { expected: Base::CODE, found: raw.base }));
        }
        if raw.quote != Quote::CODE {
            return Err(to_de_error(&WireError::WrongCurrency { expected: Quote::CODE, found: raw.quote }));
        }
        rate_from_amount(raw.rate.as_ref()).map_err(|e| to_de_error(&e))
    }
}

// ---------------------------------------------------------------------------------------
// Opt-in mode: TRANSPARENT
// ---------------------------------------------------------------------------------------

/// One scalar instead of an object: `"USD 10.50"`, `"USD/IDR/16000"`.
///
/// Use per field: `#[serde(with = "kamu_money_core::wire::transparent")]`. Binary is identical to the
/// default form — the same `(ISO numeric, units)` tagged shape (R2-F2) — because the mode a
/// field picks for humans must not change whether its bytes carry a currency.
pub mod transparent {
    use super::{
        StaticCurrency, money_from_binary, money_to_binary, rate_from_binary, rate_to_binary, to_de_error,
    };
    use crate::{Money, Rate};
    use core::str::FromStr;
    use serde::{Deserialize, Deserializer, Serializer};

    /// The seal. Unnameable downstream, so [`Scalar`] cannot be implemented outside this crate
    /// — which is what the trait's documentation has always claimed and, until it gained this
    /// supertrait, did not enforce.
    mod sealed {
        pub trait Sealed {}
        impl<C: crate::StaticCurrency> Sealed for crate::Money<C> {}
        impl<Base: crate::StaticCurrency, Quote: crate::StaticCurrency> Sealed for crate::Rate<Base, Quote> {}
    }

    /// Serialize a value as a single scalar.
    ///
    /// # Errors
    /// Propagates the serializer's own errors; rendering itself cannot fail.
    pub fn serialize<T: Scalar, S: Serializer>(value: &T, s: S) -> Result<S::Ok, S::Error> {
        value.to_scalar(s)
    }

    /// Deserialize a value from a single scalar.
    ///
    /// # Errors
    /// A wrong currency, malformed text, excess precision, or a value outside the domain,
    /// rendered through the deserializer's error type.
    pub fn deserialize<'de, T: Scalar, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
        T::from_scalar(d)
    }

    /// The crate's value types, and **actually** sealed.
    ///
    /// The seal is the `sealed::Sealed` supertrait, exactly as [`StaticCurrency`] does it.
    /// An earlier version of this comment claimed the trait was *"effectively sealed: every
    /// impl is bounded on `StaticCurrency`, which is itself sealed, so no downstream type can
    /// satisfy it"*. That was wrong, and provably so — the bound was on the **impls**, not on
    /// the trait, so a downstream `impl Scalar for MyType` compiled fine. `#[doc(hidden)]`
    /// hides a method from the docs; it does not restrict who may write one.
    ///
    /// Two things the mistake would have cost: `#[serde(with = "...transparent")]` would
    /// silently accept foreign types, and adding a method here would be a breaking change for
    /// impls this crate does not know about — the precise outcome the comment believed it had
    /// ruled out.
    pub trait Scalar: sealed::Sealed + Sized {
        #[doc(hidden)]
        fn to_scalar<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error>;
        #[doc(hidden)]
        fn from_scalar<'de, D: Deserializer<'de>>(d: D) -> Result<Self, D::Error>;
    }

    impl<C: StaticCurrency> Scalar for Money<C> {
        fn to_scalar<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if s.is_human_readable() {
                s.serialize_str(&self.to_string())
            } else {
                // The SAME tagged binary as the default form. A field's chosen human-readable
                // shape must not change whether its binary carries a currency: leaving this a
                // bare `i128` would make `#[serde(with = transparent)]` a silent hole in the
                // cross-check the default impl enforces.
                money_to_binary(*self, s)
            }
        }

        fn from_scalar<'de, D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            if !d.is_human_readable() {
                return money_from_binary(d);
            }
            let text = String::deserialize(d)?;
            Self::from_str(&text).map_err(|e| to_de_error(&e))
        }
    }

    impl<Base: StaticCurrency, Quote: StaticCurrency> Scalar for Rate<Base, Quote> {
        fn to_scalar<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            if s.is_human_readable() { s.serialize_str(&self.to_string()) } else { rate_to_binary(*self, s) }
        }

        fn from_scalar<'de, D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            if !d.is_human_readable() {
                return rate_from_binary(d);
            }
            let text = String::deserialize(d)?;
            Self::from_str(&text).map_err(|e| to_de_error(&e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MoneyIn, RateIn};
    use std::borrow::Cow;

    #[test]
    fn structured_numbers_borrow_when_the_input_needs_no_unescaping() {
        let money: MoneyIn<'_> = serde_json::from_str(r#"{"currency":"USD","amount":"10.50"}"#).unwrap();
        let rate: RateIn<'_> =
            serde_json::from_str(r#"{"base":"USD","quote":"IDR","rate":"16000"}"#).unwrap();

        assert!(matches!(money.amount, Cow::Borrowed("10.50")));
        assert!(matches!(rate.rate, Cow::Borrowed("16000")));
    }
}

/// The default form, nameable so a field can say so explicitly.
///
/// `{"currency":"USD","amount":"10.50"}` / `{"base":"USD","quote":"IDR","rate":"16000"}`.
/// Identical to the bare `Serialize`/`Deserialize` impls; this module exists so a struct
/// mixing both modes reads symmetrically instead of leaving one field's format implicit.
pub mod structured {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialize in the default structured form.
    ///
    /// # Errors
    /// Propagates the serializer's own errors.
    pub fn serialize<T: Serialize, S: Serializer>(value: &T, s: S) -> Result<S::Ok, S::Error> {
        value.serialize(s)
    }

    /// Deserialize from the default structured form.
    ///
    /// # Errors
    /// Propagates the deserializer's own errors, including the currency cross-check.
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
        T::deserialize(d)
    }
}
