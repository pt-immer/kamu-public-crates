//! `postgres-types` adapters: money in a column, as the canonical text form. (DESIGN.md C9)
//!
//! # Why text, and not `numeric`
//!
//! This is the settled answer and it is not a compromise. `numeric(36,18)` **silently rounds
//! over-precise input on the way in** — E13 measured `'0.0000000000000000004'` stored as zero,
//! with `INSERT 0 1` and no warning — and no `CHECK` or `DOMAIN` can catch it, because
//! constraints run *after* the cast and are shown the already-altered value. A type that cannot
//! be written to safely is not a storage type.
//!
//! Text has no lossy cast to hide in. It is exact over the whole domain, it carries its
//! currency so a stored amount cannot be separated from what it denominates, and it is
//! **arithmetically inert**: a `text` column has no `*`, no `/`, no `avg()`, so E9's boundary
//! rule needs no policing because the operators do not exist. That is C8's *"the boundary rule
//! disappears"* reached by a second road — and this road runs on any PostgreSQL, including the
//! managed services (RDS, Cloud SQL, Neon, Supabase) that will not load a native extension.
//! (`YugabyteDB` DOES load it natively now — E16 — so on YB the native type serves, not this road.)
//!
//! Cost, stated rather than buried: wider on disk than `numeric` for typical amounts, and
//! unordered without a functional index. E14 already established that this design does not
//! compete on bytes.
//!
//! # One codec, four consumers
//!
//! The bytes written here are exactly what [`Display`](core::fmt::Display) prints, what the
//! serde wire carries (C7), and what `kmoney`'s in/out functions read (C8). Not a matching
//! format — the *same* function. `kamu_money_core::text` is the only place this crate turns money
//! into characters, which is why a value written by an application and a value written by the
//! extension cannot disagree.
//!
//! ```no_run
//! # use kamu_money_core::{Money, iso::USD};
//! # fn f(client: &mut postgres::Client) -> Result<(), Box<dyn std::error::Error>> {
//! client.execute("CREATE TABLE ledger (amount text NOT NULL)", &[])?;
//! let paid = Money::<USD>::from_major(10).unwrap();
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
//! On a server with `kamu-money-pg` installed the column can be the native type instead, and then
//! **the cast is part of the query rather than an implementation detail**. These adapters accept
//! text-family OIDs and reject the `kmoney` OID before any parsing (R2-F5, deliberately — there
//! is no native binary codec), so the parameter travels as text and the *server* converts. That
//! runs the extension's own input function, which is this same codec, so nothing is re-implemented
//! at the boundary.
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
//! let paid = Money::<USD>::from_major(10).unwrap();
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
//! A view or a named projection is the usual way to avoid repeating the casts. Both halves are
//! executed against a live extension through **both** drivers by
//! `kamu-money-core/tests/pg_native_column.rs` (`just test-pg-driver`) — the write half since
//! 2026-07-27, which is when it stopped being an inference.

use crate::currency::StaticCurrency;
use crate::money::Money;
use crate::rate::Rate;
use bytes::BytesMut;
use core::str::FromStr;
use postgres_types::{FromSql, IsNull, ToSql, Type, to_sql_checked};
use std::error::Error;

/// The column types these adapters will read and write: **exactly** what `&str` accepts.
///
/// # Reading a native `kmoney` column requires an explicit cast
///
/// This rejects by **OID, before any parsing**, so `kamu-money-pg`'s native `kmoney` type is not
/// accepted here no matter which wire format the server would emit. That is deliberate (R2-F5:
/// no native-OID binary codec), but it makes the query shape part of the contract rather than
/// an implementation detail:
///
/// ```sql
/// SELECT amount::text FROM ledger;   -- decodes into Money<C>
/// SELECT amount        FROM ledger;  -- ERROR: the kmoney OID is not text-family
/// ```
///
/// "The driver reads a native column as text" means a **server-side cast changes the OID**, not
/// that the client negotiates a text format. A view or a named projection is the usual way to
/// avoid repeating the cast.
///
/// Notably **not** `NUMERIC` — accepting it would let a schema silently move to the one
/// storage type this design rejects, and the failure would appear as a rounded amount rather
/// than as a type error. Delegating to `&str` preserves that refusal: `postgres-types` does
/// not accept `NUMERIC` for `&str` either.
///
/// This used to hand-list `TEXT | VARCHAR | BPCHAR`, which quietly made the two drivers
/// disagree about what a money column is: [`crate::sqlx_pg`] delegates to `&str`, whose sqlx
/// impl also covers `NAME`, `citext` and — the one that bites — `UNKNOWN`, the type
/// PostgreSQL assigns a parameter it cannot infer. So a query that worked with a `&str` bound
/// failed with `Money<C>`, in a pair of modules whose entire thesis is that they are the same
/// codec. Two adapters that each round-trip correctly can still disagree with each other;
/// this is that hazard in the type list rather than in the digits.
fn accepts_to_sql(ty: &Type) -> bool {
    <&str as ToSql>::accepts(ty)
}

/// The read direction, delegating to `&str`'s FROMSQL impl — which is deliberately a
/// different set from the write direction's. `postgres-types` lets `&str` be written to
/// `ltree`/`lquery`/`ltxtquery` but not read from them, and mirroring that exactly is the
/// point: the claim is "wherever a `&str` goes, a `Money<C>` goes", and a single shared
/// predicate would have quietly made it "almost".
fn accepts_from_sql(ty: &Type) -> bool {
    <&str as FromSql>::accepts(ty)
}

impl<C: StaticCurrency> ToSql for Money<C> {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        // `to_string` is Display, which is the canonical form by definition. Going through it
        // rather than re-rendering here is what makes the "one codec" claim structural.
        <&str as ToSql>::to_sql(&self.to_string().as_str(), ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_to_sql(ty)
    }

    to_sql_checked!();
}

impl<'a, C: StaticCurrency> FromSql<'a> for Money<C> {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let text = <&str as FromSql>::from_sql(ty, raw)?;
        // `FromStr` checks the currency against `C` as well as parsing the digits, so a row
        // written as IDR cannot be read into a `Money<USD>` — the cross-check that catches a
        // column being read as the wrong currency, where types alone cannot help.
        Ok(Self::from_str(text)?)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_from_sql(ty)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> ToSql for Rate<Base, Quote> {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        <&str as ToSql>::to_sql(&self.to_string().as_str(), ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_to_sql(ty)
    }

    to_sql_checked!();
}

impl<'a, Base: StaticCurrency, Quote: StaticCurrency> FromSql<'a> for Rate<Base, Quote> {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let text = <&str as FromSql>::from_sql(ty, raw)?;
        // Checks BOTH ends of the pair, base first — accepting a reversed pair would invert
        // the price, which is the one error a quote feed can make that still looks like a
        // number.
        Ok(Self::from_str(text)?)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_from_sql(ty)
    }
}
