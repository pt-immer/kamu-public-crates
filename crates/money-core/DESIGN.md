# kamu-money-core — current contract

This document states the invariants implemented by `kamu-money-core` and the
database boundary that consumes it. It is a contract, not a development diary.
Tests and types carry the executable proof; changelogs carry history.

The public API is intentionally telescopic:

1. Start at the crate root with `Money`, `Rate`, `Rounding`, `Division`, and
   `Residue`.
2. Use `allocation`, `locale`, `text`, `wire`, and `adapters` for a specific
   boundary.
3. Enter `advanced` only for raw-unit kernels, database integration, or stable
   hashing.

## Governing rule

> A stored amount is the truth. Formatting may pad it; no boundary may silently
> round, truncate, saturate, redenominate, or discard it.

The rule has three consequences:

- currency identity travels with every runtime boundary;
- lossy operations require an explicit rounding or residue decision;
- database adapters reuse the same parser and arithmetic kernels as Rust code.

## Contract summary

| Contract | Owner | Core promise |
| --- | --- | --- |
| [C1](#c1--canonical-representation) | `domain`, `money` | One checked `i128` representation at scale 18 |
| [C2](#c2--currency-register) | `iso`, build script | One closed, generated ISO 4217 register |
| [C3](#c3--static-currency-identity) | `currency`, `Money<C>` | Cross-currency arithmetic does not type-check |
| [C4](#c4--exact-arithmetic-and-conservation) | `arith`, `allocate` | Exact arithmetic; order-independent sum; conserving allocation |
| [C5](#c5--division-and-residue) | `Division`, `Residue` | Quotient stays bundled until residue is handled |
| [C6](#c6--typed-fx-rates) | `Rate<Base, Quote>` | Positive typed rates and explicit conversion failure |
| [C7](#c7--text-and-wire-formats) | `text`, `wire` | Canonical text and tagged serde forms |
| [C8](#c8--postgresql-boundary) | adapters, `extensions/money-pg` | Text portability or one validated native payload |
| [C9](#c9--driver-adapters) | `adapters` | Thin `postgres-types` and sqlx integrations |
| [C10](#c10--integer-conversion-and-unsafe-policy) | lints, constructors | Checked narrowing; no unsafe in the core crate |

### C1 — Canonical representation

`Money<C>` stores one private `i128` count of canonical units:

```rust
pub struct Money<C: StaticCurrency> {
    units: i128,
    // zero-sized currency marker
}
```

- `SCALE` is 18. One unit is `10^-18` of the currency's major unit.
- `DOMAIN_MAX` is `10^36 - 1` canonical units. Valid values satisfy
  `abs(units) <= DOMAIN_MAX`, giving a major-unit magnitude below `10^18`.
- Scale is not a field. Values cannot carry different scales.
- `Money<C>` is 16 bytes; the marker is zero-sized.
- `units()` exposes the checked value for integration. Reconstruction goes
  through `try_from_units` or another checked constructor.
- `try_from_major` checks multiplication and the money domain.
- Construction outside the domain returns `AmountError`; it never clamps.

The representation deliberately does not use `rust_decimal::Decimal`. The money
domain needs the full checked `i128` range at a fixed structural scale, while
intermediate multiplication needs a wider type. `ethnum::I256` is used only for
intermediates and accumulators; it is not a second storage representation.

### C2 — Currency register

The ISO 4217 pipeline is:

```text
vendor/list-one.xml
    -> build/iso4217.rs
    -> OUT_DIR/iso4217.rs
    -> iso::Iso4217 and generated marker types
```

- The vendored register is the sole source of currency codes, numeric
  identifiers, names, and settlement exponents.
- The build validates source identity and register consistency before emitting
  Rust.
- The generated set is closed. Unknown alpha-3 or numeric codes are errors.
- Each generated marker implements sealed `StaticCurrency`; downstream crates
  cannot mint a counterfeit marker.
- ISO settlement exponent and locale display policy are separate concepts.
  `LocalePolicy` may choose presentation digits, but it may not discard
  non-zero canonical units.
- Currencies without an ISO exponent retain `None`; the API does not invent
  fractional semantics for them.

Update the XML, `NOTICE`, and `VENDORED.md` together. Never hand-edit generated
tables.

### C3 — Static currency identity

`Money<USD>` and `Money<IDR>` are distinct Rust types. `Add`, `Sub`, `Rate`, and
allocation APIs preserve that identity in their signatures.

```rust,compile_fail
use kamu_money_core::{Money, iso::{IDR, USD}};

let usd = Money::<USD>::try_from_major(1).unwrap();
let idr = Money::<IDR>::try_from_major(1).unwrap();
let _ = usd + idr;
```

There is no runtime-currency `Money` variant. A boundary that discovers a code
at runtime must validate it and produce a statically typed value, or keep its raw
form private until the caller names the expected type.

This restriction is deliberate: a “dynamic money” type with arithmetic invites
calculation before currency identity has been proved. Runtime heterogeneity is a
schema or application-boundary concern, not a second arithmetic model.

### C4 — Exact arithmetic and conservation

| Operation | Contract |
| --- | --- |
| `checked_add`, `checked_sub` | Exact; `None` only when the result leaves the money domain |
| `+`, `-` | Exact; panic on domain overflow, matching integer operator convention |
| unary `-` | Total for valid money because the domain excludes `i128::MIN` |
| `try_sum` | Accumulate in `I256`, check each term, narrow once |
| `allocate` | Return owned parts whose exact sum equals the input |
| `split` | Lazily yield equal parts while distributing every remaining unit |

`Money<C>` intentionally does not implement `Sum`. Folding through `+` makes
the answer depend on traversal order when an intermediate leaves the domain but
the final total returns to it. `try_sum` uses `UnitSum`, so it is a function of
the multiset rather than its order or a database query plan.

Raw-unit kernels in `advanced::arithmetic` validate their operands. Their public
`i128` inputs do not inherit `Money<C>`'s constructor proof.

Allocation validates weights, rejects zero total weight, and distributes the
integer remainder deterministically. It has no residue because all units remain
in the returned parts.

### C5 — Division and residue

Integer division returns one `Division<C>`, not a tuple:

```rust
fn take_residue(self) -> (Money<C>, Residue<C>);
fn discard_deliberately(self) -> Money<C>;
```

The quotient cannot escape until the caller chooses one exit. Dropping an
unresolved `Division` is harmless because no quotient was released.

A taken `Residue<C>` is `#[must_use]`. It can be consumed with `take_units()` or
explicitly discarded. The type never panics from `Drop`: Rust has no linear
types, and a panic-on-drop policy would turn cancellation or a second unwind
into an operational failure without adding a compile-time guarantee.

For every rounding mode:

```text
quotient * divisor + residue == original canonical units
```

Use `allocate` for payment splitting. `div_int` models division and exposes its
remainder; it does not promise that repeated quotients reconstruct the input.

### C6 — Typed FX rates

`Rate<Base, Quote>` is a private `i128` count at the same scale as money.
`Rate<USD, IDR>` means the price of one USD expressed in IDR.

- A rate is strictly positive and within the shared domain.
- `try_from_units` owns this invariant. Text, serde, postgres-types, and sqlx
  ingress all converge on it.
- `Money<Base>::convert` returns `Money<Quote>`.
- Mismatched currency pairs fail to compile.
- Conversion accepts an explicit `Rounding` and returns `Result`; ordinary
  in-domain inputs can still produce an out-of-domain result.
- There is no `Mul<Rate>` operator because a fallible operator would hide that
  normal failure mode.
- `convert_via` multiplies both rate legs in `I256` and rounds once at the end.
  It does not materialize a bridge-currency balance.

There is no `inverse()`: real FX has distinct bid and ask quotes. There is no
`compose()`: a composed mid-rate is a new quote the holder does not possess.
Callers that need a two-leg conversion use `convert_via`.

Conversion does not return `Residue`. Its discarded remainder is below one
canonical unit, so no integer number of money units exists to return. Integer
division differs because its residue can contain whole canonical units.

### C7 — Text and wire formats

Canonical text carries alpha-3 identity:

```text
USD 10.50
USD/IDR/16000
```

Rendering starts from all 18 canonical digits, removes only trailing zeroes, and
retains the ISO settlement exponent as a minimum where one exists. It never
rounds. `Display`, serde's transparent mode, database text storage, and the
native extension's input/output functions share this codec.

With feature `serde`, human-readable formats offer two field-level modes:

| Mode | `Money<USD>` | `Rate<USD, IDR>` |
| --- | --- | --- |
| Structured, default | `{"currency":"USD","amount":"10.50"}` | `{"base":"USD","quote":"IDR","rate":"16000"}` |
| Transparent adapter | `"USD 10.50"` | `"USD/IDR/16000"` |

Wire mode is selected per field, not by a Cargo feature. Cargo features unify
across a dependency graph and therefore cannot safely choose one consumer's
serialization format.

Non-human-readable serde always carries identity:

- money: `(ISO numeric u16, i128 units)`;
- rate: `(base u16, quote u16, i128 units)`.

Decoding validates every code, expected currency, rate sign, and money domain.
The numeric code is the ISO discriminant, never an enum ordinal that can move
when the register changes.

### C8 — PostgreSQL boundary

Two storage routes serve different deployment constraints.

#### Portable text

The `postgres` and `sqlx` features encode canonical text such as
`"USD 10.50"`. Text is exact, carries currency, has no arithmetic operators, and
works on managed services that cannot load native extensions.

The adapters do not encode `NUMERIC`. PostgreSQL can round over-precise numeric
input before constraints observe it, and driver numeric codecs can introduce a
second decimal representation. Canonical text keeps parsing in this crate.

#### Native `kmoney`

The excluded [`extensions/money-pg`](../../extensions/money-pg) workspace owns a
pgrx extension for self-hosted PostgreSQL and YugabyteDB:

- one SQL type per ISO 4217 currency — `kmoney_idr`, `kmoney_usd`, and the
  rest — derived from this crate's register; each supports typed arithmetic,
  and a cross-currency expression fails while the query is parsed;
- `kmoney_mixed` stores heterogeneous currencies and deliberately has no
  arithmetic or sum aggregate;
- a pinned payload is 16 little-endian unit bytes — the currency lives in the
  catalog, not the value; `kmoney_mixed` appends two little-endian ISO-code
  bytes for 18;
- payload validation is centralized before semantic use;
- the Rust representation uses byte arrays with alignment 1, avoiding an
  unaligned `i128` reference at the PostgreSQL ABI boundary;
- required unsafe code is isolated below `src/ffi/`; `src/safe/` remains
  ordinary safe Rust.

The extension reuses `kamu-money-core` parsing, arithmetic, allocation, and
stable-hash kernels. Its own [`DESIGN.md`](../../extensions/money-pg/DESIGN.md)
and [YugabyteDB runbook](../../extensions/money-pg/kamu-money-pg/yb/RUNBOOK.md)
own deployment and ABI details.

### C9 — Driver adapters

The `postgres` and `sqlx` adapters live in `kamu-money-core` because Rust's
orphan rule prevents a sibling crate from implementing a foreign driver trait
for `Money<C>`.

Both adapters are deliberately thin:

1. accept the driver's text-compatible PostgreSQL type;
2. call the canonical text renderer or parser;
3. reconstruct through checked `Money` or `Rate` constructors.

They do not copy currency tables, domain rules, or rate validation. Native
`kmoney` columns require an explicit server-side text cast for these portable
adapters. A bare native OID is rejected rather than guessed.

Feature-gated container tests cover scalar and array round trips for
`postgres-types` and sqlx. The extension lane separately tests native-column
binary behavior against PostgreSQL 15–18 and YugabyteDB.

### C10 — Integer conversion and unsafe policy

The core crate denies unsafe code and denies conversion lints that commonly hide
money loss.

- Widening uses `From` or another infallible conversion.
- Narrowing returns `Option`/`Result`, or uses `try_from(...).expect(...)` beside
  a local proof of totality.
- Domain checking occurs at every public raw-unit ingress.
- Lossless re-encoding is total: `i128 <-> [u8; 16]` and
  `I256 <-> [u8; 32]` use explicit little-endian functions.
- Saturation is not a money policy. No current API silently clamps a value.
- `as` conversions are denied except for narrowly scoped, documented ABI cases
  in the extension lane.
- An `#[allow]` is scoped to the smallest statement or item and states why it is
  sound and when it can be removed.

The extension's unsafe warranty is narrower than the semantic contract: unsafe
code may translate PostgreSQL ABI values, but it must hand validated owned or
byte-backed values to safe kernels before arithmetic. Miri does not prove the
foreign ABI; live catalog, scalar, array, binary, and multi-version database
tests complement it.

## Verification map

| Property | Primary executable evidence |
| --- | --- |
| Domain, exact arithmetic, wide sum | `src/{domain,arith}.rs` tests; `tests/conservation.rs` |
| Conserving allocation and lazy split | `src/allocate.rs`; `tests/conservation.rs` |
| Currency closure and generated register | `tests/register_codegen.rs`; build-script validation |
| Static currency and residue API shape | `tests/compile_fail.rs`; `tests/ui/` |
| Canonical text and locale non-loss | `tests/text.rs`; `tests/locale.rs` |
| Tagged serde and wrong-currency refusal | `tests/wire.rs` |
| Positive rates at every ingress | `tests/rate_ingress.rs` |
| PostgreSQL text adapters | `tests/pg_roundtrip.rs`; `tests/sqlx_roundtrip.rs` |
| Native extension payload and ABI | `extensions/money-pg/hygiene`; PostgreSQL/YugabyteDB suites |

## Rejected alternatives

| Alternative | Reason rejected |
| --- | --- |
| `rust_decimal::Decimal` as storage or compute representation | Does not cover the canonical domain while preserving a fixed structural scale |
| Runtime-currency `Money` with fallible arithmetic | Allows calculation before currency identity is proved |
| `Iterator::sum()` through `Add` | A transient out-of-domain partial makes results order-dependent |
| PostgreSQL `NUMERIC` storage | Over-precise ingress can round before constraints inspect it |
| Separate public driver-adapter crates | Orphan rules prevent the required trait implementations |
| Untagged binary `i128` | Lets bytes written as one currency decode as another |
| Cargo-feature-selected wire format | Feature unification silently couples unrelated consumers |
| `Rate::inverse()` or `Rate::compose()` | Fabricates trade semantics or a quote the holder does not possess |
| Native pgrx crate in the public workspace | Would impose patches, profiles, toolchain, and database builds on unrelated crates |
