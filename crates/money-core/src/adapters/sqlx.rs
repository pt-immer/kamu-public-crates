//! `sqlx` PostgreSQL adapters for the canonical text form.
//!
//! Values render on output, parse on input, and reject non-text columns. Cross-driver tests
//! verify that `sqlx` and `postgres-types` share the same representation.
//!
//! # Why this is a feature and not a `money-sqlx` crate
//!
//! Rust's orphan rule requires the adapter impls to live in the crate that owns `Money<C>`.
//!
//! # Why `sqlx`'s own `NUMERIC` support is not used
//!
//! PostgreSQL `numeric` rounds over-precise input before constraints can inspect it. Its sqlx
//! representation also has a narrower decimal range than this crate.

use super::codec::{decode, decode_money, encode};
use crate::{Money, Rate, StaticCurrency};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef, Postgres};
use sqlx::{Decode, Encode, Type};

/// Money is carried as `text`. Declaring the type this way — rather than by OID — is what makes
/// `compatible` accept `varchar` and `bpchar` too.
///
/// It also means a native `kamu-money-pg` per-currency column is **not** readable directly: `compatible`
/// is consulted against the column's OID before any parsing, so the query must cast —
/// `SELECT amount::text`, never a bare `SELECT amount`. See
/// `adapters::postgres` for the full note; the two adapters share this boundary
/// exactly.
impl<C: StaticCurrency> Type<Postgres> for Money<C> {
    fn type_info() -> PgTypeInfo {
        <str as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        // Text family only. `numeric` is excluded on purpose: accepting it would let a schema
        // drift onto the one storage type this design rejects, and the failure would surface as
        // a rounded amount rather than a type error.
        <str as Type<Postgres>>::compatible(ty)
    }
}

impl<C: StaticCurrency> PgHasArrayType for Money<C> {
    fn array_type_info() -> PgTypeInfo {
        <&str as PgHasArrayType>::array_type_info()
    }
}

impl<C: StaticCurrency> Encode<'_, Postgres> for Money<C> {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(encode(self).as_str(), buf)
    }
}

impl<C: StaticCurrency> Decode<'_, Postgres> for Money<C> {
    fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
        let text = <&str as Decode<Postgres>>::decode(value)?;
        Ok(decode_money(text)?)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Type<Postgres> for Rate<Base, Quote> {
    fn type_info() -> PgTypeInfo {
        <str as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <str as Type<Postgres>>::compatible(ty)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> PgHasArrayType for Rate<Base, Quote> {
    fn array_type_info() -> PgTypeInfo {
        <&str as PgHasArrayType>::array_type_info()
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Encode<'_, Postgres> for Rate<Base, Quote> {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(encode(self).as_str(), buf)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Decode<'_, Postgres> for Rate<Base, Quote> {
    fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
        let text = <&str as Decode<Postgres>>::decode(value)?;
        Ok(decode(text)?)
    }
}
