//! `postgres-types` adapters for the canonical text form.
//!
//! # Why text, and not `numeric`
//!
//! PostgreSQL `numeric(36,18)` rounds over-precise input before `CHECK` or `DOMAIN`
//! constraints can inspect it. Text preserves the crate's reject-instead-of-round contract.
//!
//! Text also carries the currency and exposes no accidental arithmetic operators. The tradeoff
//! is wider storage for typical amounts and no numeric ordering without a functional index.
//!
//! # One codec, four consumers
//!
//! Both Rust drivers encode through [`Display`](core::fmt::Display) and decode through
//! [`FromStr`](core::str::FromStr). Serde and `kmoney` share the same parser and renderer.
//!
//! ```no_run
//! # use kamu_money_core::{Money, iso::USD};
//! # fn f(client: &mut postgres::Client) -> Result<(), Box<dyn std::error::Error>> {
//! client.execute("CREATE TABLE ledger (amount text NOT NULL)", &[])?;
//! let paid = Money::<USD>::try_from_major(10).unwrap();
//! client.execute("INSERT INTO ledger VALUES ($1)", &[&paid])?;
//!
//! let row = client.query_one("SELECT amount FROM ledger", &[])?;
//! let back: Money<USD> = row.get(0);
//! assert_eq!(back, paid);
//! # Ok(()) }
//! ```
//!
//! # The native `kmoney` column: one canonical projection, both directions
//!
//! With `kamu-money-pg`, cast explicitly at the query boundary. These adapters accept
//! text-family OIDs and reject the native `kmoney` OID before parsing.
//!
//! Read `amount::text`; write `($1::text)::kmoney`. That is the whole contract:
//!
//! ```no_run
//! # use kamu_money_core::{Money, iso::USD};
//! # fn f(client: &mut postgres::Client) -> Result<(), Box<dyn std::error::Error>> {
//! // `kmoney('USD')` pins the column to one currency; the typmod coercion enforces it, so a
//! // bound `Money<IDR>` is refused by the DATABASE rather than by a convention.
//! client.execute("CREATE TABLE ledger_native (id int primary key, amount kmoney('USD'))", &[])?;
//!
//! let paid = Money::<USD>::try_from_major(10).unwrap();
//! client.execute(
//!     "INSERT INTO ledger_native (id, amount) VALUES ($1, ($2::text)::kmoney)",
//!     &[&1i32, &paid],
//! )?;
//!
//! // Arithmetic happens in the database, over the same Rust kernel the client uses.
//! client.execute(
//!     "UPDATE ledger_native SET amount = amount + (($1::text)::kmoney) WHERE id = $2",
//!     &[&paid, &1i32],
//! )?;
//!
//! let row = client.query_one("SELECT amount::text FROM ledger_native WHERE id = 1", &[])?;
//! let back: Money<USD> = row.get(0);
//! assert_eq!(back, paid + paid);
//! # Ok(()) }
//! ```
//!
//! A view or named projection can centralize repeated casts. `just test-pg-driver` exercises
//! both directions through both drivers against a live extension.

use super::codec::{decode, encode};
use crate::{Money, Rate, StaticCurrency};
use bytes::BytesMut;
use postgres_types::{FromSql, IsNull, ToSql, Type, to_sql_checked};
use std::error::Error;

/// The column types these adapters will read and write: **exactly** what `&str` accepts.
///
/// # Reading a native `kmoney` column requires an explicit cast
///
/// This rejects by OID before parsing, so a native `kmoney` column requires an explicit cast:
///
/// ```sql
/// SELECT amount::text FROM ledger;   -- decodes into Money<C>
/// SELECT amount        FROM ledger;  -- ERROR: the kmoney OID is not text-family
/// ```
///
/// The server-side cast changes the OID; the client does not negotiate a native text format.
/// Delegating to `&str` keeps accepted types aligned with `postgres-types` and excludes
/// `NUMERIC`.
fn accepts_to_sql(ty: &Type) -> bool {
    <&str as ToSql>::accepts(ty)
}

/// Preserve `postgres-types`' distinct read and write type sets.
fn accepts_from_sql(ty: &Type) -> bool {
    <&str as FromSql>::accepts(ty)
}

impl<C: StaticCurrency> ToSql for Money<C> {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        <&str as ToSql>::to_sql(&encode(self).as_str(), ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_to_sql(ty)
    }

    to_sql_checked!();
}

impl<'a, C: StaticCurrency> FromSql<'a> for Money<C> {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let text = <&str as FromSql>::from_sql(ty, raw)?;
        Ok(decode(text)?)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_from_sql(ty)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> ToSql for Rate<Base, Quote> {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        <&str as ToSql>::to_sql(&encode(self).as_str(), ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_to_sql(ty)
    }

    to_sql_checked!();
}

impl<'a, Base: StaticCurrency, Quote: StaticCurrency> FromSql<'a> for Rate<Base, Quote> {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let text = <&str as FromSql>::from_sql(ty, raw)?;
        Ok(decode(text)?)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_from_sql(ty)
    }
}
