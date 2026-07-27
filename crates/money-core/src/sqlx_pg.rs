//! `sqlx` adapters for PostgreSQL: the same canonical text form as [`crate::pg`]. (specs.md C9)
//!
//! Deliberately a thin restatement of the same three moves — render on the way out, parse on
//! the way in, refuse anything that is not a text column. There is no second codec here, and
//! the tests assert that a value written by `sqlx` reads back through `postgres-types` and
//! vice versa, which is the only way "one codec" stays true rather than becoming a comment.
//!
//! # Why this is a feature and not a `money-sqlx` crate
//!
//! C9 asked for a separate crate. `impl Type<Postgres> for Money<C>` from an external crate is
//! **E0117** — foreign trait, foreign type — verified with a throwaway compile, not recalled.
//! The only workaround is a newtype the caller spells at every boundary. Feature-gating in the
//! crate that owns the type is what `serde` already does here, and what `chrono` and `uuid` do
//! generally.
//!
//! # Why `sqlx`'s own `NUMERIC` support is not used
//!
//! It decodes to `Decimal`, which reintroduces the E5 ceiling on the wire — the exact reason
//! `rust_decimal` was removed as a dependency. And `numeric` cannot be written to safely at
//! all: E13 measured PostgreSQL silently rounding over-precise input on the way *in*, where no
//! `CHECK` or `DOMAIN` can reach it.

use crate::currency::StaticCurrency;
use crate::money::Money;
use crate::rate::Rate;
use core::str::FromStr;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo, PgValueRef, Postgres};
use sqlx::{Decode, Encode, Type};

/// Money is carried as `text`. Declaring the type this way — rather than by OID — is what makes
/// `compatible` accept `varchar` and `bpchar` too.
///
/// It also means a native `kamu-money-pg` `kmoney` column is **not** readable directly: `compatible`
/// is consulted against the column's OID before any parsing, so the query must cast —
/// `SELECT amount::text`, never a bare `SELECT amount`. See [`crate::pg`] for the full note; the
/// two adapters share this boundary exactly.
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
        // `to_string` is `Display`, which IS the canonical form. Going through it rather than
        // re-rendering here is what makes the one-codec claim structural instead of aspirational.
        <&str as Encode<Postgres>>::encode(self.to_string().as_str(), buf)
    }
}

impl<C: StaticCurrency> Decode<'_, Postgres> for Money<C> {
    fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
        let text = <&str as Decode<Postgres>>::decode(value)?;
        // `FromStr` checks the currency against `C` as well as the digits, so a row written as
        // IDR cannot be read into a `Money<USD>`.
        Ok(Self::from_str(text)?)
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
        <&str as Encode<Postgres>>::encode(self.to_string().as_str(), buf)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Decode<'_, Postgres> for Rate<Base, Quote> {
    fn decode(value: PgValueRef<'_>) -> Result<Self, BoxDynError> {
        let text = <&str as Decode<Postgres>>::decode(value)?;
        // Checks BOTH ends of the pair — accepting a reversed one would invert the price.
        Ok(Self::from_str(text)?)
    }
}
