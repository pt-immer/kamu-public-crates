//! One scalar instead of an object: `"USD 10.50"`, `"USD/IDR/16000"`.
//!
//! Use per field: `#[serde(with = "kamu_money_core::wire::transparent")]`. Binary is identical to the
//! default form — the same `(ISO numeric, units)` tagged shape — because the mode a
//! field picks for humans must not change whether its bytes carry a currency.

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

/// The crate's sealed scalar value types.
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

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::iso::{IDR, USD};
    use serde::{Deserialize, Serialize};

    /// The transparent mode's binary form is tagged too — otherwise `#[serde(with = transparent)]`
    /// would be a silent hole in exactly the cross-check the default form now enforces.
    #[test]
    fn transparent_binary_also_refuses_a_cross_currency_reinterpretation() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct U(#[serde(with = "crate::wire::transparent")] Money<USD>);
        #[derive(Deserialize)]
        struct I(
            #[serde(with = "crate::wire::transparent")]
            #[allow(dead_code)]
            Money<IDR>,
        );

        let bytes = postcard::to_allocvec(&U(Money::<USD>::try_from_major(10).unwrap())).unwrap();
        assert!(
            postcard::from_bytes::<I>(&bytes).is_err(),
            "transparent binary must reject a mismatched currency too"
        );
        assert_eq!(postcard::from_bytes::<U>(&bytes).unwrap(), U(Money::<USD>::try_from_major(10).unwrap()));
    }
}
