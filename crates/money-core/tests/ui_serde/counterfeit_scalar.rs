//! `wire::transparent::Scalar` must be sealed, so `#[serde(with = "...transparent")]` cannot
//! be pointed at a foreign type.
//!
//! This case exists because the trait's documentation claimed to be sealed while it was not.
//! The bound was on the impls (`impl<C: StaticCurrency> Scalar for Money<C>`), not on the
//! trait, so this file COMPILED — and the compile-fail suite had no case that would notice.
//! An untested compile error is a claim that silently stops being true.

use serde::{Deserializer, Serializer};

struct Counterfeit;

impl kamu_money_core::wire::transparent::Scalar for Counterfeit {
    fn to_scalar<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i128(0)
    }

    fn from_scalar<'de, D: Deserializer<'de>>(_d: D) -> Result<Self, D::Error> {
        Ok(Self)
    }
}

fn main() {}
