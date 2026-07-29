# kamu-money-core

[![CI][badge-ci]][link-ci]
[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Exact monetary arithmetic: `i128` at a fixed scale of 18, with compile-time
currency identity. Division withholds its quotient until the caller explicitly
takes or discards the residue.

> Version `0.1.0` is present in the repository; its first crates.io release is
> pending.

The crate is intentionally ISO 4217-only. `StaticCurrency` is sealed so wire,
database, and arithmetic identity all use the same generated register.

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

## The one rule that explains the rest

> **Truth is stored.** If the store says `10.00` it shows `10.00`. Display is a
> string, and it pads — it never rounds.

A `Money<USD>` is exactly 16 bytes. The currency lives in the type and only in
the type, so `Money<USD> + Money<IDR>` is a compile error rather than a runtime
one. `+` and `-` cannot round. Division can, and it hands back a `Division` that
will not give up its quotient until you say what happens to the remainder.

## Scope

- **The scalar** — `Money<C>`, `i128` canonical units at scale 18, bounded to the
  magnitude of PostgreSQL's `NUMERIC(36,18)`.
- **Currency identity** — a sealed `StaticCurrency` per ISO 4217 code, generated
  at build time. Downstream crates cannot mint a counterfeit currency.
- **Rates** — `Rate<Base, Quote>`, strictly positive, with no `inverse()` and no
  `compose()` because real bid/ask semantics make both misleading.
- **Allocation and division** — conserving splits, and an explicit `Residue`.
- **Display** — `LocalePolicy`, deliberately *not* `Display`, and deliberately
  not a locale database.

**Out of scope, and not a defect:** any ledger, journal or account schema,
idempotency store, connection pool, or service boundary. This is an amount
scalar for a consuming schema, never a store.

## Features

| Feature    | Default | Description                                                        |
| ---------- | ------- | ------------------------------------------------------------------ |
| `serde`    | no      | Structured `Serialize`/`Deserialize`, plus a transparent-string adapter. |
| `postgres` | no      | `ToSql`/`FromSql` for the `postgres-types` driver stack.           |
| `sqlx`     | no      | `Encode`/`Decode`/`Type` for `sqlx`'s PostgreSQL driver.           |

The database adapters live in this crate rather than in sibling adapter crates
because `impl ToSql for Money<C>` from an external crate is `E0117` — a foreign
trait on a foreign type. That is the same reason `serde` is a feature here, and
the same choice `chrono` and `uuid` make.

## API map

Start at the crate root: `Money`, `Rate`, `Iso4217`, `Rounding`, `Division`,
`Residue`, and `MoneyError`.

| Need | Path |
| ---- | ---- |
| ISO currency markers | `iso` |
| Locale display and canonical text | `locale`, `text` |
| Split iterator and narrow errors | `allocation`, `errors` |
| Raw units, bounds, residue internals, stable hashing | `advanced` |
| Serde formats | `wire` |
| PostgreSQL driver traits | `adapters::postgres`, `adapters::sqlx` |

The earlier flat module paths remain as deprecated, documentation-hidden
migration shims through `0.1.x`. They are scheduled for removal in `0.2.0`;
compiler notes name each replacement.

## Allocation conserves the total

```rust
use kamu_money_core::{Money, iso::USD};

let whole = Money::<USD>::try_from_major(10).unwrap();
let parts = whole.allocate(&[1, 1, 1]).unwrap();

assert_eq!(parts.iter().map(|p| p.units()).sum::<i128>(), whole.units());
```

Ten split three ways loses nothing. The remainder is distributed, not discarded.

## A division has exactly two exits

Lossy division returns a `Division`, not a number. Obtaining its quotient
requires taking the residue or discarding it on the record. Dropping the
unresolved `Division` is safe because no quotient escaped.

```rust
use core::num::NonZeroU32;
use kamu_money_core::{Money, Rounding, iso::USD};

let whole = Money::<USD>::try_from_major(10).unwrap();
let three = NonZeroU32::new(3).unwrap();

// Exit one: take it, and it is yours to place.
let (each, residue) = whole.div_int(three, Rounding::TowardZero).take_residue();
assert_eq!(residue.take_units(), 1, "10 into 3 leaves 1 unit at scale 18");

// Exit two: say out loud that you do not want it.
let same = whole
    .div_int(three, Rounding::TowardZero)
    .discard_deliberately();
assert_eq!(each, same, "the two exits agree on the quotient");
```

A taken `Residue` is `#[must_use]`, but Rust does not have linear types: callers
can suppress that lint. It never panics in `Drop`, so cancellation and unwinding
cannot turn an accounting mistake into a process abort.

## Currency data

The ISO 4217 register is generated at build time from
[`vendor/list-one.xml`](vendor/list-one.xml), published by SIX Group AG as the
maintenance agency for ISO 4217. There is no committed table to hand-edit and no
generator to forget to run: the XML and the table are the same object, and the
publication date, row counts and internal consistency are checked as it is read,
so a replaced register fails the build rather than a test.

Credit, provenance and the redistribution position are in
[`VENDORED.md`](VENDORED.md) and [`NOTICE`](NOTICE).

## Design

[`DESIGN.md`](DESIGN.md) carries the current C1–C10 contract and points each
invariant to executable evidence. It also records the alternatives deliberately
excluded from the API.

## Licence

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.

`vendor/list-one.xml` is **not** covered by that licence. It is third-party data,
redistributed unmodified with credit to SIX Group AG; see [`NOTICE`](NOTICE).

[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.94-blue?style=flat-square&logo=rust

[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/money-core
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
