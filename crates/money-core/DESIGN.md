# kamu-money-core — Money Type Contract

**Status:** implemented and tested — the exact scalar and its arithmetic (C1–C7, C10), the
`postgres` / `sqlx` adapters (C9), and `LocalePolicy` (C2). The native PostgreSQL type (C8) is
green on PostgreSQL 15–18 and native plus byte-exact on YugabyteDB (E16); it lives in a separate
extension lane and is not part of this crate.
**Date:** 2026-07-17, last revised on re-homing into `kamu-public-crates`
**Toolchain:** edition 2024, MSRV 1.94

> **Scope.** This document carries the classification, the evidence base, the contracts and the
> rejected alternatives — the reasoning a consumer of `kamu-money-core` needs. The workspace
> layout (§3), the test contract (§4), the open items (§6) and the phasing (§7) describe the
> PostgreSQL extension lane and travel with it rather than with this crate. Section numbering is
> preserved rather than renumbered, so every `C`/`E` citation in the source keeps resolving.

---

> Known context is evidence, not permission. Every binding below cites a measurement.
> Claims without evidence are marked `UNVERIFIED` and must be measured before they are relied upon.

---

## 0. Classification

| Cell | Binding |
|---|---|
| **WHAT** | A monetary quantity: a signed multiple of `1e-18` of an ISO-4217 currency unit, bounded by `\|v\| < 10^18`. |
| **HOW** | Exact integer arithmetic for `+`/`-`; explicitly-rounded division that returns its `Residue` (`div_int`, `allocate`). There is deliberately **no** general money multiplication — the only scaling operation is FX `convert`/`convert_via`, which takes an explicit `Rounding` and returns **no** `Residue` (see **WHEN**). |
| **WHERE** | Three surfaces: Rust process memory, a native PostgreSQL type (**15–18**, and native + byte-exact on YugabyteDB, E16), and a serde wire (JSON string / **tagged** binary — `(u16, i128)` for `Money`, `(u16, u16, i128)` for `Rate`, never a bare int; R2-F2). |
| **WHO** | Rust services (currency in the type, always); frontend clients (TUI/Web/Mobile) as string consumers; PostgreSQL as a first-class arithmetic peer, not a dumb store: `kmoney` has `+`, `-`, comparison operators (as predicates), a stable `kmoney_hash`, binary I/O and a typmod. `kmoney` is an amount **scalar** for OLTP wallet/ledger schemas — a column type, not a store: this workspace implements no account, transaction, journal or balance, and rows being keyed by account/txn is the assumed usage of the consuming schema rather than a guarantee made here. On that assumption it deliberately carries **no** B-tree or hash operator class. The resulting limitation stands independently of the assumption: no default sort operator, no value index, no `ORDER BY`/`GROUP BY`/`DISTINCT`/`UNIQUE` by amount, so a consumer needing those (OLAP) needs another projection or type. Summing a COLUMN is `sum(kmoney)`, an aggregate with a **wide** (`I256`) transition state, order-independent and genuinely `PARALLEL SAFE`; summing explicit VALUES is `kmoney_sum(VARIADIC kmoney[])`. R2-F4 removed an earlier aggregate whose state was a plain `kmoney` — the narrow state was the defect, not the aggregate (R2-F4b). Rust keeps `Money::try_sum` in place of `Sum`, because a fold through `+` is inherently narrow. |
| **WHEN** | Rounding occurs only at named sites, never implicitly, and every remainder **representable in canonical units** is surfaced as a `Residue`. The one loss that is not: FX `convert` may discard a **sub-canonical** fraction after an explicit `Rounding`, because a fraction below `1e-18` cannot be expressed as a nonzero `Residue`. "Never without returning the residue" is simpler and false; this is the real contract. |
| **WHY** | Rounding and precision loss in money are *silent* by default in every layer of the default stack. This type makes every loss either impossible, or loud, or accounted for. |

### 0.1 The axiom

> **Truth = stored. Calculate with the truth. Calc in Rust == calc in PG. Showing is a string.**
> *(Operator, verbatim: "If the store is 10.00 then it must show 10.00, no discrepancy.")*

This is not a preference. It is the premise every contract below is derived from, and it **decides**
questions rather than informing them. Anything that introduces a *second number claiming to be the
money* violates it, whatever the second number is for.

It has already killed three separately-motivated proposals, all of which were the same violation
wearing different clothes:

| Proposal | Why the axiom rejects it |
|---|---|
| `to_minor_units() -> i64` for payment rails | A projection to another *money* representation, lossy. Two numbers claim to be the money and one silently isn't. Rails get `quantize(dp, mode) -> (Money, Residue)` **at the adapter**, which eats the residue where the loss happens. |
| `allocate_at_exponent(EXP)` | Presupposes a settlement boundary. **This system has none.** See §0.2. |
| Rounding in display | `3.333333333334 → "$3.33"` is a lie about the stored truth. Display **pads, never rounds** — see C7. |

When one stated axiom independently rejects three proposals with different motivations, it is
load-bearing rather than stylistic. Treat it as such.

### 0.2 The penny problem does not exist here

Fowler's `3.33 × 3 = 9.99` bug exists **because 2dp is where money lives** in the systems it was
described for. Here, **18dp is where money lives**: the schema *defines* money as a multiple of
`1e-18`. `allocate(10.00, [1,1,1])` returns three exact, storable parts summing to exactly
`10.000000000000000000`. No cent evaporates, because there is no cent.

The bug requires a **settlement boundary** — a point where money must be expressed at a currency's
minor unit to leave the system. Phases 1–5 contain no such boundary. An earlier revision of this
document claimed for three consecutive turns that phase 1 "does not close the bug it exists to
close." **That was wrong**: it asserted a defect in a component that does not exist, by importing
conventional framing into a design that rejects its premise.

Conventional wisdom is a prior, not a requirement. Check whether the premise holds *here* before
importing the remedy.

### 0.3 The thesis

Complexity is justified here **only** where it deletes a failure that can be demonstrated on command.
Every addition below cites the failure it kills. Every deletion cites the measurement that killed it.

**Three of this design's fixes have been deletions** — `from_parts_unchecked` (speculative, zero
callers), `to_minor_units()` (a lossy convenience), and `StaticCurrency::EXP` (a duplication whose
only consumer was the test guarding it). In an explicit over-engineering experiment, the design got
*more* rigorous every time it got *smaller*. The complexity that survived kills a demonstrable
failure; the complexity that died was decoration that looked like rigor.

### 0.4 Why pgrx at all, and why so much effort on YugabyteDB

Two questions any newcomer asks, and the two most expensive commitments in this workspace. Both
follow from §0.1 rather than from taste, and both now have a **measured** price rather than a
rationale. Answering them here, first, because the answers are load-bearing: someone who does not
know them will eventually propose deleting one.

#### Why a native extension, and why pgrx rather than C

§0.1 says *calc in Rust == calc in PG*. Not "close enough" — identical. Everything else follows.

**Why an extension at all.** If PostgreSQL cannot compute money, there are two options and both are
worse:

1. *The application does all arithmetic and the database is a dumb store.* Then
   `UPDATE account SET balance = balance - :amt WHERE id = :id` is unavailable, and every transfer
   becomes read-compute-write — a race invented on purpose, needing application locking to undo. On
   a distributed store that race spans nodes.
2. *The database computes with `numeric`.* That is a **second implementation of money**. E9 and E13
   record what PostgreSQL's `numeric` actually does; it rounds by rules the Rust code does not
   share, carries no currency, and E2/E3 show what happens when two decimal implementations
   disagree quietly. Two implementations of money arithmetic do not stay equal. They drift, and the
   drift is silent and is about money.

**Why pgrx rather than hand-written C.** A C extension would be a *third* implementation — its own
domain check, its own parser, its own allocator, its own `I256` widening for E7's multiply. C9
exists to prevent exactly that: **one codec, one kernel, one set of rules.** pgrx is what lets the
SQL surface be a thin adapter over `kamu-money-core` — `kmoney_add` calls `add_units`,
`kmoney_allocate` calls `allocate_units`, `sum(kmoney)` folds `UnitSum`, the input function calls
`text::parse`. §0.1's equality is then true **by construction** rather than by a differential test
suite someone must maintain over the full domain forever.

It also keeps `#![deny(unsafe_code)]` over the money logic. The C version would be unsafe
throughout, at the layer that must not be wrong.

**What that costs, now measured (E20).** A pgrx call is ~376–395 ns against a native C function's
~31 ns. So correctness-by-construction costs roughly **350 ns per boundary crossing** — and the
lever is call *count*, not the arithmetic, which is already faster than `rust_decimal`. That is the
whole trade, stated as a number: one implementation instead of two, for ~350 ns a crossing.

> **Corrected 2026-07-26 (second correction to this section).** The paragraph above is wrong, and
> the trade is better than it claims. That ~350 ns was measured against a **debug** build of the
> extension — `cargo pgrx install` defaults to debug — and compared against `numeric`, which is
> PostgreSQL's own release-built C. Re-measured in release, **a pgrx call costs single-digit
> nanoseconds**, and `kmoney` is about *twice as fast* as `numeric` at arithmetic. See E20's
> retraction block for the isolation.
>
> So the price of correctness-by-construction is not ~350 ns a crossing; on the evidence there is
> no per-crossing price worth naming. The argument for pgrx never depended on the number being
> small — it is that two implementations of money drift — but this section quoted a figure as *the
> whole trade*, and the figure was an artefact. Left visible rather than overwritten, because a
> reader who finds the old number quoted elsewhere needs to know it was retracted here.

#### Why YugabyteDB, and why proving it took this much

**Why YugabyteDB is a target at all.** The deployment reality is RF3 (three primaries, three read
replicas) and RF5 (five and five) — because a payment system cannot answer "the primary is down"
with an apology, and cannot answer growth with a bigger box. YugabyteDB gives PostgreSQL wire and
SQL compatibility over Raft-replicated distributed ACID. It is *the deployment target*, not an
experiment, which is why the operator directive makes it first-class rather than a fallback.

**Why that required work rather than an assumption.** YugabyteDB is a PostgreSQL **fork**, not
PostgreSQL — patched sources, its own clang, a multi-threaded YSQL. An extension built against
stock PostgreSQL can fail there in two ways, and they are not equally well evidenced here. Saying
which is which is the point of this paragraph:

- **Measured, and loud.** E15: YugabyteDB's own `elog.h` includes `ybc_util.h`, which the image
  does not ship, so *every* extension fails to compile; and a PGDG-built `.so` copied in fails the
  loader on `GLIBC_2.30`. Today an unadapted build still fails, for a third loud reason — pgrx
  refers to `CurrentMemoryContext`, YB's bindings expose only `YbCurrentMemoryContext`, and a name
  that does not exist is a link error, not a wrong answer. Every directly evidenced failure on this
  path is a build or load failure.
- **The risk model, and NOT measured here.** *It builds, loads, runs, and is subtly wrong.* No
  retained run in this repository shows that outcome, and the sentence that used to sit here
  asserted it as though one did. What makes it a real class rather than a scare story: a fork
  compiled with a different toolchain need not preserve stock struct layouts even when the loader
  is satisfied; and the two memory-context names are one release away from coexisting, at which
  point `CurrentMemoryContext` resolves to a *process-global* inside a *multi-threaded* YSQL and
  nothing raises. `probe-yb-abi.sh` guards exactly that — it requires `YbCurrentMemoryContext` to
  exist **and** no upstream `CurrentMemoryContext` extern to have returned beside it — but a guard
  against a hazard is not a demonstration of it.

Both belong in the argument, and only the first is a measurement. The reason "it passes on
PostgreSQL" is not evidence about YugabyteDB is that the second class **cannot be ruled out by
reasoning** — which is an argument for proving the adapted path on the real engine, not a claim
that the unadapted one was watched corrupting a number. So: E16 (native, byte-exact against stock
PG15), E17 (the whole `#[pg_test]` contract, on YB), E18 (three nodes, every node, a tablet split,
200 concurrent double-entry transfers), a read-replica placement, the image digest pin, and an ABI
probe that reads the headers before anything compiles. Those measure the **adapted path behaving
correctly**, which is the claim this workspace is entitled to make.

> **Corrected 2026-07-26**, after an external review. This section shipped on 2026-07-25 with the
> silent-corruption outcome stated flatly as the causal centrepiece of the architecture — an
> unmeasured failure carrying a load-bearing argument, in a document whose own header says claims
> without evidence are marked `UNVERIFIED` and must be measured before they are relied upon. The
> conclusion did not change; the warrant for it did. Retaining that distinction is worth more than
> the sharper sentence, because the next reader who checks E15 against this paragraph must find
> them agreeing.

**What the alternative was.** Without that work the choice is: deploy an unverified extension onto
money, or retreat to the text adapter and lose in-database arithmetic — which lands straight back
on option 1 above, the race we refused to invent.

#### Both answers are the same answer

This is §0.3's rule applied at the largest scale in the workspace. pgrx deletes *"two
implementations of money that drift"*. The YugabyteDB harness deletes *"we assumed a fork behaves
like its upstream"*. Neither is decoration; each kills a failure that can be demonstrated on
command, and each now carries the number it costs.

---

## 1. Evidence Base

All figures measured during design, not recalled. Reproduce before trusting.

### E1 — `rust_decimal` 1.42.1 domain

```text
Decimal::MAX  = 79228162514264337593543950335   -> 29 significant digits
Decimal::MAX_SCALE = 28
```

### E2 — `Decimal::from_str` silently corrupts

```text
from_str      ("999999999999999999999999.999999999999")  -> Ok("1000000000000000000000000.0000")
from_str_exact("999999999999999999999999.999999999999")  -> Err(Underflow)
```

The input is the maximum value of `NUMERIC(36,12)`. It returns as a **different number, carrying into a 25th integer digit.**

```text
from_str      ("0.000000000000000000000000000001") -> Ok("0.0000000000000000000000000000")   // ZERO
from_str_exact("0.000000000000000000000000000001") -> Err(Underflow)
```

A nonzero amount silently becomes zero.

### E3 — `Decimal::checked_add` silently drops scale, returns `Some`

Accumulating values of order 9e15 (Indonesia M2 magnitude, in IDR) at scale 12:

```text
 8 x M2 = 72000000000000000.000000000000   scale=12
 9 x M2 = 81000000000000000.00000000000    scale=11   <-- SILENT. returned Some(_), not None.
```

The cliff is the **96-bit mantissa** (`2^96 - 1 ≈ 7.92e28`), not a digit count, so it is ragged and value-dependent:
`8 x M2` needs mantissa 7.2e28 (fits); `9 x M2` needs 8.1e28 (does not) → scale silently reduced.

**Consequence: a `scale == 12` invariant is unmaintainable on `Decimal`. Ordinary addition breaks it, silently, at IDR-realistic magnitudes.**

### E4 — Domains are incomparable, both directions

```text
PG NUMERIC(36,12) max  = 999999999999999999999999.999999999999   (36 sig digits)  -> Decimal CANNOT hold
Decimal can hold        99999999999999999999999999               (26 int digits)  -> PG (36,12) REJECTS (cap 24)
from_str_exact("100000000000000000.000000000000")  -> Err(Underflow)
    ^ 1e17 IDR at 12dp. 18 integer digits. PERFECTLY LEGAL in NUMERIC(36,12). Decimal cannot represent it.
```

### E5 — The "Decimal as compute lens" idea is dead

```text
Decimal::try_from_i128_with_scale(10^36 - 1, 12) -> Err(ExceedsMaximumPossibleValue)
lens ceiling = 79228162514264337 currency units
domain max   = 999999999999999999999999 currency units
=> covers 0.000007923% of the i128 domain
```

### E6 — `i128 @ scale 12` spans the domain exactly

```text
domain max units = 999999999999999999999999999999999999   (10^36 - 1)
i128::MAX        = 170141183460469231731687303715884105727 (~1.7e38)
fits = true, headroom = 170x
```

### E7 — `i128` is exact but not total; multiply needs widening

```text
i128::MAX.checked_add(1)      -> None
i128::MIN.checked_neg()       -> None
(10^36-1).checked_mul(1.5e12) -> None      // 1e36 * 1.5e12 = 1.5e48 >> 1.7e38
```

### E8 — Measured sizes

```text
i128        = 16 bytes, align 16
(i128, ())  = 16 bytes      -> Money<USD>
(i128, u16) = 32 bytes      -> what Money<Dyn> cost   (i128's 16-byte alignment doubles it)
Decimal     = 16 bytes
```

### E9 — PostgreSQL 18.4 `numeric` semantics

Measured against a throwaway `postgres:18` container on a loopback port, column type `numeric(36,12)`.

| Operation | Result | Verdict |
|---|---|---|
| `0.000000000001 + 0.000000000002` | `0.000000000003`, scale **12** | **exact, bit-identical to `i128@12`** |
| `1.5 * 2.5` | `3.750000000000000000000000`, scale **24** | exact, but wrong scale → silent round on store |
| `1::numeric / 3::numeric` | scale **20** | PG chooses |
| `10.00::numeric(36,12) / 3` | scale **16** | **PG chooses differently for the same types** |
| `round(0.5), round(1.5), round(2.5), round(3.5)` | `1, 2, 3, 4` | **half-away-from-zero**, not half-even |
| `INSERT 0.0000000000025` into `numeric(36,12)` | stored `0.000000000003` | **silent**; half-even gives `...002` |
| `avg(1,2,2)` | `1.6666666666666667`, scale 16 | rounds, no residue |
| `pg_typeof(sum(x))`, `pg_typeof(avg(x))` | `numeric`, `numeric` | **unconstrained** — may exceed the column domain |
| `INSERT 1e25` into `numeric(36,12)` | `ERROR: numeric field overflow` / *"must round to an absolute value less than 10^24"* | **loud** |

**Structural finding:** PostgreSQL independently lands on the *same seam* as the `i128` design — add/sub exact, mul/div lossy. The seam is a property of decimal arithmetic, not of either implementation.

**PG's division scale is value-dependent, not type-dependent** (`1/3` → 20; `10.00::numeric(36,12)/3` → 16). PG's internal `select_div_scale()` keys off operand *weights*. Therefore PG's division rounding **cannot be mirrored from Rust** by any rounding mode or configuration. Matching it would mean forking PG's numeric internals bug-for-bug and re-verifying against every future major.

### E13 — PostgreSQL 18.4 `numeric(36,18)`, measured (2026-07-22)

Measured against a throwaway `postgres:18` container (18.4) on a loopback port. **This closes the open item**, which stood as `UNVERIFIED` because E9 quotes PG only for
`numeric(36,12)`.

| Probe | Result |
|---|---|
| store `999999999999999999.999999999999999999` | accepted, stored exact |
| store `10^18` | `ERROR: numeric field overflow` / `DETAIL: A field with precision 36, scale 18 must round to an absolute value less than 10^18.` |
| `1e-18 + 2e-18` | `0.000000000000000003`, scale **18** — exact and scale-preserving |
| `sum()` over two domain-top rows | `1999999999999999999.999999999999999998`, type `numeric` (unconstrained) |

The derived bound was correct: `|v| < 10^18`, in PG's own words. C1's claim that `SUM()` widens
past the column type is likewise now measured rather than asserted.

**But the same probe found something the derivation could not — and it is the more important
half.**

#### PostgreSQL silently rounds over-precise input. Exactly as `rust_decimal` did (E2)

```text
'0.0000000000000000005'::numeric(36,18)          ->  0.000000000000000001
'0.0000000000000000004'::numeric(36,18)          ->  0.000000000000000000   <-- ZERO
'0.00000000000000000049999999999'::numeric(36,18)->  0.000000000000000000   <-- ZERO
```

`INSERT` returns `INSERT 0 1`. No error, no warning. **A nonzero amount silently becomes zero** —
the verbatim failure that disqualified `rust_decimal` in E2, reproduced by the database this
design treats as its source of truth.

#### No constraint can catch it, because constraints run AFTER the cast

```text
CREATE TABLE guarded (v numeric(36,18) CHECK (v <> 0));
INSERT INTO guarded VALUES ('0.0000000000000000004');
    ERROR:  new row for relation "guarded" violates check constraint
    DETAIL:  Failing row contains (0.000000000000000000).      <-- already rounded

INSERT INTO guarded VALUES ('0.0000000000000000005');
    INSERT 0 1                                                 <-- 1e-18. CHECK passes.
```

A `CHECK` can reject the round-**to-zero** case, by rejecting zero. It cannot detect the
round-**to-nonzero** case at all: `5e-19` arrives as `1e-18`, which is a perfectly legal value
that simply is not the one that was sent. A `DOMAIN` behaves identically — it also runs after
the cast.

**Consequence, and it retroactively justifies a decision that was argued only on taste.**
`kamu-money-core` refusing over-precise input (`MoneyError::ExcessPrecision`) is not merely the
safer choice: the application boundary is **the only place this loss can be caught at all**.
The database cannot protect itself, and every write path that bypasses `kamu-money-core` — an
ad-hoc `INSERT`, a migration, an ETL job, another service on the same column — can silently
alter or zero an amount.

This is a **bounded** exception to §0.1's *"calc in Rust == calc in PG"*: the two agree on
arithmetic, and disagree on **ingestion of values that were never representable**. Rust refuses;
PG rounds. Named here rather than discovered later.

### E10 — Dependency versions (checked 2026-07-17)

| Crate | Version | Note |
|---|---|---|
| `pgrx` | 0.19.1 | features `pg13`…`pg19` — **pg18 supported** |
| `ethnum` | 1.5.3 | has **signed** `I256` |
| `sqlx` | 0.9.0 | |
| `postgres-types` | 0.2.14 | |
| `proptest` | 1.11.0 | |
| `serde` | 1.0.228 | already in tree |

`ruint`, `primitive-types`, `crypto-bigint` are **rejected: unsigned-only.** They cannot hold a negative balance.

### E11 — Spine prototype, compiled and run (rustc 1.97.0, edition 2024)

The `iso4217!` macro, the currency traits, `Money<C>`, exact `Add`/`Sum`, the domain invariant, and the
`ethnum::I256` multiply path were prototyped and executed before this plan was written. Output:

```text
Usd add       : 12750000000000
Usd code      : USD  (via blanket)
Sum           : 23250000000000
Xau::EXP      : None
drift check   : true                       // Iso4217::X.exponent() == X::EXP for every generated currency
// NOTE: `X::EXP` no longer exists. The line is the prototype's actual output and is
// left as run; the const it names was deleted afterwards, which made the drift it checked
// unrepresentable. Evidence records what happened, not what is currently true.
Dyn code      : IDR
sizes         : Money<USD>=16  Money<Dyn>=32
domain edge   : true / false               // DOMAIN_MAX ok, DOMAIN_MAX+1 rejected

i128 checked_mul(DOMAIN_MAX, 1.5e12) = None
I256 product  = 1499999999999999999999999999999999998500000000000
I256 quotient = 1499999999999999999999999999999999998   (fits i128: true)
I256 residue  = 500000000000
```

Three claims tested in isolation:

| Test | Result |
|---|---|
| `impl<C: StaticCurrency> CurrencyRepr for C` + `impl CurrencyRepr for Dyn` | **COMPILES.** No E0119. Coherence rules out the overlap because `Dyn` and `StaticCurrency` are both crate-local, so the orphan rule already forbids a downstream `impl StaticCurrency for Dyn`. |
| `struct Money<C: CurrencyRepr> { units: i128, tag: C::Tag }` without `PhantomData` | **COMPILES.** `C::Tag` alone counts as a use of `C`. |
| `Money::<Usd> + Money::<Idr>` | **Correctly rejected**, `error[E0308]: mismatched types`. |

**`#[derive(Clone/Copy/PartialEq)]` on `Money<C>` is wrong** and must be hand-written: derive emits
`impl<C: CurrencyRepr + Clone> Clone for Money<C>`, bounding `C` when the bound belongs on `C::Tag`.

### E12 — pgrx's varlena risk, resolved by reading pgrx's source (not by spiking, not by memory)

An earlier revision carried this as an open risk: *"`#[derive(PostgresType)]` defaults to serde+CBOR
varlena, which would destroy the memcpy claim justifying `kmoney`."* Read from `pgrx 0.19.1`:

```text
pgrx-macros/src/lib.rs:914   Some(unsafe { ::pgrx::datum::cbor_encode(&self) }.into())
pgrx-macros/src/lib.rs:873   ... && !args.contains(&PostgresTypeAttribute::PgVarlenaInOutFuncs)
pgrx-macros/src/lib.rs:822   `pgvarlena_inoutfuncs(..)`: custom in/out functions for the `PgVarlena` of this type
pgrx-macros/src/lib.rs:833   bikeshed_postgres_type_manually_impl_from_into_datum
pgrx/src/datum/varlena.rs:95 pub struct PgVarlena<T> where T: Copy + Sized
```

**The CBOR claim was TRUE. The conclusion drawn from it was FALSE.** CBOR is *conditional*, and there
are two escape hatches: `pgvarlena_inoutfuncs` and
`bikeshed_postgres_type_manually_impl_from_into_datum`. `PgVarlena<T> where T: Copy + Sized` is
precisely `#[repr(C)] struct RMoney { units: i128, code: u16 }`.

Residual cost: `PgVarlena` is still varlena-framed, so an 18-byte payload carries PG's **1-byte short
header** → 19 bytes. It never TOASTs (threshold ~2KB), and decode is read-header-then-cast. **That is
the memcpy.** C8's economics hold.

> **The lesson is sharper than "measure, don't recall".** Here the recalled premise was *correct* and
> the inference was still wrong, because the search was incomplete — the way out sat two lines below
> the code being half-remembered. A true premise does not license an inference. And reading the
> library's source cost two commands, versus the 20-minute container spike this document previously
> prescribed and the hours it spent as unearned FUD in the open items.

#### E12a — correction to E12, found while implementing it (2026-07-22)

E12 originally read, verbatim:

> CBOR is *conditional* — explicitly gated on not using `pgvarlena_inoutfuncs`.

**That attribution is wrong**; the paragraph above has been corrected in place and the original is
preserved here. Both lines E12 quoted are real, but they gate **different things**:

```text
pgrx-macros/src/lib.rs:872-876   if !InOutFuncs && !PgVarlenaInOutFuncs {
                                     // assume the user wants us to implement the InOutFuncs
                                     args.insert(PostgresTypeAttribute::Default); }
pgrx-macros/src/lib.rs:908       if !args.contains(&PostgresTypeAttribute::ManualFromIntoDatum) {
pgrx-macros/src/lib.rs:911-914       impl IntoDatum ... cbor_encode(&self)
pgrx-macros/src/lib.rs:931-941       impl FromDatum ... cbor_decode(datum.cast_mut_ptr())
```

pgrx's own comment on line 875 settles it: the 872 gate chooses which **text** `in`/`out` functions to
emit (`Default` / `InOutFuncs` / `PgVarlenaInOutFuncs`, dispatched at 983 / 1001 / 1024). The CBOR
`IntoDatum`/`FromDatum` pair — the one that decides how the value crosses a **datum** boundary — is
gated at 908 on `ManualFromIntoDatum`, which is a *different attribute*, set by a *different* derive
option (line 1249).

**Consequence:** `#[pgvarlena_inoutfuncs]` alone leaves the CBOR `IntoDatum` in place. C8's memcpy
claim would have been quietly false for every datum-passed value — function arguments, returns, index
entries — while remaining true for the text form the tests read. `kamu-money-pg` therefore carries **both**
attributes, with `IntoDatum`/`FromDatum` hand-written to delegate to `PgVarlena`.

There is also a **third** `cbor_decode` at line 1076, inside the `send`/`recv` binary-protocol impls
gated on `PgBinaryProtocol` (line 1049). We do not opt in, so it is not emitted — but a future
`COPY BINARY` or binary-mode client path must not enable that attribute expecting a memcpy.

> **E12 committed the error it diagnoses, one level down.** It correctly ruled that a true premise does
> not license an inference — then quoted a real line and inferred the wrong gate from it. Reading two
> lines is not reading the function. The failure survived because the *conclusion* was right
> (`pgvarlena_inoutfuncs` is indeed part of the answer) and nothing downstream tested the reasoning:
> a wrong derivation reaching a right answer leaves no failing test behind. It surfaced only when the
> code was written, where the missing attribute became `Serialize`/`Deserialize` bound errors.

### E14 — `kmoney`, finally **measured** (2026-07-22, PostgreSQL 18.4 PGDG, pgrx 0.19.1, rustc 1.97.0)

E12 predicted 19 bytes by reading source. The first `pg_column_size` returned **36**. Every number below
comes from `kamu-money-pg`'s `#[pg_test]` suite, run in a container against a PGDG PostgreSQL.

**1. `i128` cannot be a field of a PostgreSQL varlena struct.** Measured with `size_of`/`align_of`:

| layout | `size_of` | `align_of` |
|---|---:|---:|
| `#[repr(C)] { units: i128, code: u16 }` | **32** | **16** |
| `#[repr(C)] { units: [u8; 16], code: [u8; 2] }` | **18** | **1** |

E12's "18-byte payload" added field widths and ignored alignment. `i128` is 16-byte aligned on x86-64,
so the struct is padded to 32 and the measured column was 36.

**2. The padding was the harmless half.** pgrx emits `CREATE TYPE kmoney (INTERNALLENGTH = variable,
STORAGE = extended)` with **no `ALIGNMENT` clause**, so PostgreSQL applies its `int4` default and places
the datum on a 4-byte boundary. `PgVarlena::as_ref` is `vardata_any(self.varlena.ptr) as *const T` — a
bare pointer cast, no copy, no alignment fixup. A `&kmoney` whose type demands 16-byte alignment was
therefore being manufactured from 4-byte-aligned memory: **undefined behaviour**, and a fault rather than
a slowdown wherever a 16-byte load must be aligned. PostgreSQL cannot express 16-byte alignment at all —
`double` (8) is its maximum — so no `CREATE TYPE` parameter could have rescued the `i128` layout. Byte
arrays drop `align_of` to 1, which every placement satisfies. Both `size_of == 18` and `align_of == 1`
are now `const` assertions, so the regression is a compile error rather than a test.

**3. In-memory and on-disk were different numbers, until the varlena went away.** The size moved three
times, and each move was a measurement correcting a claim this document had already made:

| layout | stored | in-memory datum |
|---|---:|---:|
| `#[repr(C)] { i128, u16 }`, pgrx varlena | 36 | 36 |
| byte arrays, pgrx varlena | 19 | **22** |
| byte arrays, `INTERNALLENGTH = 18` | **18** | **18** |

The middle row is the trap: a varlena carries a **4-byte** header in memory and PostgreSQL only repacks
it into the 1-byte short form during *tuple formation*. So `pg_column_size` on an expression said 22
while the stored column said 19 — both correct, answering different questions. Any on-disk claim has to
be measured on a stored row.

The third row removes the question. pgrx's derive emits `INTERNALLENGTH = variable` as a **hardcoded
string literal** (`pgrx-sql-entity-graph-0.19.1`, `postgres_type/entity.rs`), so every derived type is a
varlena — but PostgreSQL does not require that. `uuid` is `typlen = 16, typbyval = f, typalign = c,
typstorage = p`: fixed-length, 1-byte-aligned, plain, **not** a varlena. `kmoney` is now the same
shape two bytes wider, verified against `pg_type` rather than inferred:

```text
typlen=18   typbyval=f   typalign=c   typstorage=p
```

Reaching it meant owning the datum path — hand-written `CREATE TYPE`, `IntoDatum`, `FromDatum`,
`SqlTranslatable`, `ArgAbi`, `BoxRet` — and two ordering constraints that PostgreSQL enforces: exactly
one `extension_sql!` may carry `bootstrap`, and only a type's own in/out functions may reference it
while it is still a shell (everything else fails with `type "kmoney" is only a shell`).

**What this does not buy is pass-by-value.** `typbyval` requires `typlen <= 8`, because a `Datum` is 8
bytes; 128 bits of units plus a currency is 18. `uuid`, `interval` and `point` are all pass-by-reference
for the same reason, so a `palloc` per function result is inherent to any wide PostgreSQL type rather
than a cost this design chose — and it is a bump allocation in a per-tuple memory context that is reset
wholesale, not a `malloc`.

**4. `kmoney` is NOT a space win, and C8 previously implied it was.** `numeric(36,18)` is
variable-width and drops trailing zeros. Measured, stored:

| stored value | `kmoney` | `numeric(36,18)` |
|---|---:|---:|
| `0` | 18 | **3** |
| `0.000000000000000001` | 18 | **5** |
| `10.50` | 18 | **7** |
| `999999999999999999.999999999999999999` | **18** | 23 |

`numeric` wins everywhere except the top of the domain — by roughly 3× on a typical ledger amount, and
that comparison already flatters `kmoney`, which carries its currency inside its 18 bytes while a
`numeric` column needs a companion currency column beside it.

> **A test that measures only the favourable point is not evidence.** The first version of this
> comparison asserted `kmoney < numeric` at the domain top *only* — the single row in the table above
> where that holds — and passed. It would have gone into the record as "measured". The honest case for
> C8 is what the other tests show: a value that cannot be stored without its currency, a width that does
> not move with the data, and a refusal of the precision `numeric` silently swallows (E13). Space is not
> on that list, and any claim that it is should be treated as unmeasured.

### E15 — YugabyteDB 2025.2.4.1 cannot host `kamu-money-pg` (measured 2026-07-22, `yugabytedb/yugabyte:2025.2.4.1-b4`)

**The PostgreSQL base version, verified rather than assumed:**

```text
PostgreSQL 15.12-YB-2025.2.4.1-b0 on x86_64-pc-linux-gnu,
  compiled by clang version 19.1.0 (https://github.com/yugabyte/llvm-project.git ...), 64-bit
```

So YugabyteDB 2025.2.x is **PG15-based**, which `kamu-money-pg` already supports. That is the last
encouraging fact in this entry.

The image looks like it can host an extension: it ships `pg_config`, the server headers under
`/home/yugabyte/postgres/include/server`, and the standard `share/extension` and `lib`
directories, with ~40 extensions already installed. Both routes in are nevertheless closed, for
**two independent reasons**.

**1. The shipped headers do not compile.** YugabyteDB's own `elog.h` includes a header it does
not distribute:

```text
/home/yugabyte/postgres/include/server/utils/elog.h:20:10:
  fatal error: 'yb/yql/pggate/util/ybc_util.h' file not found
```

`find / -name ybc_util.h` returns nothing, and there is no `yb/yql` include tree anywhere in the
image. Every PostgreSQL extension includes `postgres.h`, which reaches `elog.h`, so this stops
**all** of them rather than anything specific to this one. Closing it needs the YugabyteDB
*source* tree, not the distributed image.

**2. A foreign-built `.so` will not load either.** Independent of the headers — the extension
built against PGDG PostgreSQL 15 on Debian trixie was copied straight in:

```text
ERROR:  could not load library "/home/yugabyte/postgres/lib/money_pg.so":
        /lib64/libc.so.6: version `GLIBC_2.30' not found (required by ...money_pg.so)
```

The image ships **glibc 2.28**; the build needed 2.30. That one is *soluble* — build on a base
matching YugabyteDB's glibc — but solving it only
returns you to blocker 1, and leaves the deeper question untouched: YugabyteDB's `postgres` is a
patched fork compiled with its own clang, so stock PG15 struct layouts are not something to
assume are ABI-compatible even when the loader is satisfied.

Reproduce both with `just yb-probe`. As of **2026-07-23** it also **asserts** them: each blocker
requires a non-zero exit status and its expected error signature, and the script exits 1 if
either stops reproducing — because that would mean this record describes a world that no longer
exists, and the adapter-only decision it justifies needs re-examining rather than re-asserting.
The paragraph below records the earlier state and remains accurate as history.

The 2026-07-22 rewrite made the probe **execute** both blockers rather than
asserting them. An external review (2026-07-22) caught that it did not: blocker one only
grepped for the missing `#include`, and blocker two printed a previously-captured loader error
as literal text and then ran `ldd --version`. Grepping a header shows a header is missing; it
does not show that compilation fails. Echoing an error message shows nothing at all.

The rewrite compiles YugabyteDB's own extracted server headers with a real compiler
(`compiler exit status: 1`, `fatal error: yb/yql/pggate/util/ybc_util.h: No such file or
directory`) and really builds a `.so` against a newer glibc, copies it into the running
container, and makes PostgreSQL `dlopen` it.

Three things the rewrite measured that the original could not:

1. **The YugabyteDB image ships no compiler** — `cc`, `gcc` and `clang` are all absent. So the
   headers cannot even be tried in place; the compile has to happen in a builder image against
   extracted headers. That is an independent reason this image cannot host an extension build.
2. **`LOAD` is not implemented at all** — `ERROR: LOAD not supported yet`. The first attempt at
   this reproduction used `LOAD` and got that back, which looks like a confirmation and is a
   different finding entirely. The library has to be reached the way an extension reaches it,
   `CREATE FUNCTION ... LANGUAGE C`, which `dlopen`s before any symbol lookup or magic-block
   check — so the glibc mismatch surfaces first, which is what makes this blocker observable.
3. Only then does the loader give the recorded refusal, in its own words rather than in ours.

**Scope, stated narrowly:** what is demonstrated is that *this image*, built the conventional
way, cannot host a third-party extension. The broader claim — that no YugabyteDB build can —
is not what was tested.

> **This is C8's one-way door arriving from an unexpected direction.** C8 already recorded that
> pgrx commits the design to self-hosted PostgreSQL because RDS, Cloud SQL, Neon and Supabase
> will not load a native extension — a *policy* limit. YugabyteDB refuses for a *technical* one,
> and the two are worth distinguishing: a managed service could change its policy tomorrow,
> whereas an undistributed header is a property of what YugabyteDB ships.
>
> The consequence is not that YugabyteDB is unsupported. It is that YugabyteDB is served by
> **phase 4** rather than phase 5: money stored in a type it already has, with every arithmetic
> operation in Rust. By §0.1's own axiom that is the *safer* half of this design — no SQL-side
> arithmetic means no second implementation that could disagree with the first.

### E16 — kamu-money-pg runs NATIVELY and byte-exact on YugabyteDB `2025.2.5.1-b1` (measured 2026-07-24)

E15 scoped itself precisely: *"what is demonstrated is that this image, built the conventional way,
cannot host a third-party extension."* E16 fills that scope. **kamu-money-pg builds, loads, and runs on
YugabyteDB `2025.2.5.1-b1` (banner `PostgreSQL 15.12-YB-2025.2.5.1-b0`), and its ABI is byte-exact
against stock PostgreSQL 15.**

- **Why the naive build failed, and the fix (the "3-symbol shim").** YB's YSQL is multi-threaded, so
  it renames the process-global `CurrentMemoryContext` to a thread-local `YbCurrentMemoryContext`.
  Upstream **pgrx 0.19.1** plus three `#[cfg(yb)]` patches — a `CurrentMemoryContext` alias, the extra
  `index_build_range_scan` arguments YB's signature takes, and zeroing
  `BackgroundWorker::bgw_oom_score_adj` — compiles and links against YB's own PG15 headers when built
  with `RUSTFLAGS="--cfg yb"`. The patches are `cfg`-gated, so without `--cfg yb` the vendored pgrx is
  byte-identical to upstream and the normal PG matrix is untouched. Applied at build time only
  (`kamu-money-pg/yb/apply-yb-shim.sh`), so the committed tree stays clean. (YugabyteDB itself vendors pgrx
  0.14.1 in-tree to ship `pg_parquet`, so hosting a pgrx extension is a supported shape, not a hack.)
- **The one kamu-money-pg source change it needed, now portable.** `kmoney_typmod_in` used
  `pg_sys::deconstruct_array_builtin`, a PG15 convenience wrapper YB omits; replaced with the timeless
  `pg_sys::deconstruct_array` primitive, verified across PG 15/16/17/18.
- **Byte-exactness, the sharpest signal.** The F3 pinned `kmoney_hash` values match on YB to the exact
  `i32`: `USD 0.00 → 702888007`, `USD 1.00 → -1388235877`, `IDR 1.00 → -129968833`,
  `USD -1.00 → 1671845669`, and `kmoney_hash == kmoney_mixed_hash` for the same payload. `send()` is
  the exact 18 bytes; a real `COPY (FORMAT BINARY)` round trip reconstructs every row through
  `kmoney_recv`; `+`/`-`, the cross-currency refusal text, `kmoney_sum`, `kmoney_allocate` (incl. the
  zero-weight guard), and the domain/precision refusals are all identical to stock PG15.
- **The A/B, and its one honest caveat.** `just yb-ab` runs the identical `abi_battery.sql` on YB and
  on a stock PG15 **built from source in the same image** (glibc matches; only the headers differ),
  then diffs. The diff is empty except for the client-invocation prefix (`ysqlsh:/tmp/probe.sql:` vs
  `psql:.../abi_battery.sql:`) — harness metadata, not a kmoney effect; the error text after it is
  byte-identical. One YB-only line is normalized with `SET client_min_messages = error`: YB warns
  about `ROWS_PER_TRANSACTION` when `COPY` targets a temp table, a notion stock PG15 lacks. Neither is
  a kamu-money-pg behaviour.
- **Reproduce:** `just yb-native` (battery on a fresh labelled YB) and `just yb-ab` (the byte-exact
  A/B). Both build FROM the YB image and are kept out of `check-all`, beside `test-pg`.

**Supersedes E15's conclusion, not its measurement.** E15's naive-build failure is real and still
reproduces (`just yb-probe`). What is superseded is the *inference* that YugabyteDB is therefore
served only by phase-4 text adapters: with the shim, native `kmoney` is a first-class YB citizen.
This satisfies the operator's YB-first-class directive for kamu-money-pg and closes **R2-F5**.

**What E16 does NOT say, and why E17–E19 exist.** E16 is a claim about **one node, one session, one
~112-line script**. It was correct and it was narrow, and the distinction mattered enough that a
production-readiness review put it first: *native `kmoney` loads on a single-node YugabyteDB and
behaves byte-identically to stock PG15 under one SQL battery* is a very different sentence from
*usable in production*. The evidence below closes the gap the review named.

### E17 — the whole `#[pg_test]` contract runs on YugabyteDB (measured 2026-07-25)

**Every `#[pg_test]` assertion executes against a live YugabyteDB, not just the ABI battery** — 54
of them on the date measured.

> The count in this entry is **54 because that is what was measured on 2026-07-25**, and an
> evidence entry is not rewritten to match a later tree. The contract has since grown to **63**
> (R2-F4b restored `sum(kmoney)` and brought its tests with it). The live number is whatever
> `COVERAGE.md` holds, which `repo_hygiene.rs` checks against `lib.rs` in both directions on every
> `just check` — so *"all of them"* is the claim under guard, and the integer is a snapshot.

- **Why they could not before.** `cargo pgrx test` manages its own PostgreSQL and cannot be pointed
  at a YB backend, so the tests that *are* this type's contract had only ever run on PGDG builds.
  The YB evidence surface was one script; everything it did not touch — typmod edge cases, `recv`'s
  refusals, allocation vectors, the mixed type — was unverified **there**.
- **What was built.** `kamu-money-pg/tests/pg_regress/` — 11 themed cases restating all 54
  assertions as `sql/` + `expected/` pairs, plus `run-suite.sh`, which feeds each case to a
  caller-supplied client **on stdin**. One runner therefore drives `docker exec -i <node> ysqlsh`
  and a local `psql` with no transport code. `COVERAGE.md` maps every test to its case, and
  `repo_hygiene.rs::the_case_suite_accounts_for_every_pg_test` fails the offline gate if any test
  lacks a row — *a skipped test silently counted as a pass is worse than an absent one.*
- **Nothing is unported.** 54 of 54. The manifest permits a `NOT-PORTABLE: <reason>` row; none is
  used.
- **Goldens are hand-authored from the literals the Rust tests already assert, and there is no
  regenerate mode.** A suite that can bless its own output certifies whatever it currently does.
- **The oracle has its own negative control.** `selftest.sh` replays a correct output through a
  fake client and then corrupts it 14 ways — one cent in a value, one letter in a refusal message,
  a refusal that stops being raised, a truncated run, two half-runs concatenated, an empty file, a
  client that died with perfect bytes, a case with no golden — and requires each to be rejected
  **for its own reason**. It needs no database, so it runs in `just check`.
- **Result:** `just test-yb-regress` → **11/11 cases byte-identical** on
  `PostgreSQL 15.12-YB-2025.2.5.1-b0`. The stock-PG15 reference runs the **same cases against the
  same goldens** inside `Dockerfile.pg15`, which is what makes a YB failure a divergence rather
  than an unfalsifiable question about the port's fidelity.

**Two harness defects the first runs exposed, both of which would have made the suite lie:**

1. **psql omits the error-location prefix on stdin and emits it with `-f`.** The first run failed
   9 of 11 cases with byte-identical message text underneath. The normalizer now *deletes* the
   prefix rather than rewriting it, so one golden serves both invocations.
2. **`docker exec` multiplexes stdout and stderr as separate streams, so a host-side `2>&1` cannot
   order them.** Expected-error lines arrived one `\echo` section late — *intermittently*, in 2 of
   11 cases, which is worse than a hard failure. Every caller now passes a client that merges
   inside the container (`bash -c 'exec ysqlsh "$@" 2>&1'`). Documented as a requirement on callers
   in `run-suite.sh`, because the next person to add a node type will hit it.

### E18 — a real cluster: three nodes, RF=3, tablets that move, transactions that collide (2026-07-25)

Closes **G2**, **G4**, and the cross-node half of **G8**. `kamu-money-pg/yb/cluster.sh` brings up a
labelled, trap-owned 3-node RF=3 cluster and asserts membership through `yb_servers()` — *n* nodes
each answering `SELECT 1` is equally consistent with *n* separate single-node clusters, which would
make every cross-node claim below vacuously true.

**`just test-yb-cluster`** — `CREATE EXTENSION` issued on **one** node and the type usable from
every node; the four pinned `kmoney_hash` values identical from every node; the full E17 suite run
through each node in turn; a value written on node 0 read back byte-identically on nodes 1 and 2
(compared as an md5 of the ordered text **plus** a hash fold **plus** the row count, because a
count or a sum survives a single corrupted payload); the 18-byte `send()` payload identical across
nodes; a forced **tablet split**, after which the same fingerprint must still hold. And a
**negative control**: a node with `kmoney.so` removed must fail loudly on first use — without it,
every probe above would be consistent with the library never having been needed.

**`just test-yb-concurrent`** — the invariant this type exists for. N workers spread across all
three nodes run balanced double-entry transfers, each a single distributed `BEGIN…COMMIT` touching
two rows on different tablets, with retryable conflicts retried by the caller and counted. Then:
the total over every account must equal the seeded total **exactly** (via `kmoney_sum`, which
accumulates in I256 and cannot lose a unit to its own arithmetic); the ledger's debits and credits
must cancel to exactly zero — an independent second check, since balances can conserve while legs
are lost; every balance must still re-parse to itself; and `ROLLBACK` must leave nothing behind.

**With a positive control for the retry path.** YugabyteDB surfaces retryable errors PostgreSQL does
not, and a run that happened to hit zero conflicts would report "conservation held under
concurrency" while never exercising them. Two `SERIALIZABLE` sessions are therefore made to contend
on one row deliberately, and **not** getting a retryable error is a failure.

### E19 — operability: the shim's pin, resilience, and the first performance numbers (2026-07-25)

> **SUPERSEDED IN PART, 2026-07-25 (operator decision): the shim is now a FORK.** The three
> adaptations live in
> [`pgrx-yugabytedb`](https://github.com/fluminis-scientiae-oraculum/pgrx-yugabytedb) — a true
> GitHub fork of `pgcentralfoundation/pgrx`, branch `yugabytedb-0.19.1`, tag `v0.19.1-yb.1` —
> behind a **`yb-pg15`** Cargo feature, patched in by `[patch.crates-io]` in the workspace root.
> `apply-yb-shim.sh` is deleted; `RUSTFLAGS="--cfg yb"` is replaced by `--features pg15,yb-pg15`.
>
> **This retires the defect described below rather than merely fixing it.** There is no text left
> to patch, so a patch that matches nothing is now a compile error by construction. The paragraphs
> that follow are kept because the defect was real and the reasoning is what motivated the fork.
>
> Three details are load-bearing and easy to get wrong. The crates keep the names `pgrx` /
> `pgrx-pg-sys` and the version `0.19.1` — `[patch.crates-io]` matches on crate *name*, and
> `cargo-pgrx` refuses to build an extension whose pgrx version differs from the CLI's own — so
> releases are distinguished by **tag**. The patch is **unconditional**, applying to the PGDG 15–18
> matrix too, because `kamu-money-pg` declares `yb-pg15 = ["pgrx/yb-pg15"]` and cargo validates that
> reference while resolving the graph whether or not the feature is enabled; this is safe precisely
> because all 33 added lines sit inside `#[cfg(feature = "yb-pg15")]`, so with the feature off the
> crates compile byte-identical to upstream `v0.19.1`. And the feature is named for the **base
> major**, not just the vendor: when YugabyteDB rebases, `yb-pg16` sits beside it instead of a bare
> `yb` silently changing meaning under everyone who enabled it.
>
> `probe-yb-abi.sh` remains, and is *not* made redundant by the fork: the compiler proves the
> adaptation applied, but only the probe proves it is still the **right** adaptation for the headers
> in front of it. A `index_build_range_scan` that dropped back to 11 parameters would compile with
> three arguments too many.

**A silent-failure defect in the shim, found and closed.** `apply-yb-shim.sh` applied two of its
three patches with `re.sub` and `str.replace`, both of which return the string **unchanged** when
nothing matches — and it then wrote that unchanged string back out and printed `patched`. A
YugabyteDB release that renamed any shimmed symbol would therefore have produced a **successful
build of an unshimmed extension**, with money read through the wrong memory-context global. Every
patch now counts its own matches, fails closed at zero, and re-reads the file to confirm no call
site was left unpatched. (The count is *not* pinned: `include.rs` carries one call site per
PostgreSQL major — 7 at pgrx 0.19.1 — so what is asserted is "at least one matched, and none
remains unpatched", which survives pgrx adding a major.)

**Three independent guards, answering three different questions** (G3):

| Guard | Question |
|---|---|
| `yb-image.sh` + `YB-PINNED.txt` | is this the image anyone **validated**? Refuses on drift unless `YB_ALLOW_DRIFT=1`. |
| `probe-yb-abi.sh` (inside the build, **before** patching) | are the **headers** still shaped the way the shim assumes? |
| `apply-yb-shim.sh` | did our change actually **take**? |

The probe asserts `YbCurrentMemoryContext` exists **and** that no upstream `CurrentMemoryContext`
extern has returned beside it — the alias would silently shadow a process-global with a
thread-local — that `index_build_range_scan` still takes 14 parameters (parsed and counted, not
grepped), and that `BackgroundWorker` still carries `bgw_oom_score_adj`.

**`just yb-resilience`** (G5, partial) — a node restarts, and every payload and pinned hash is
intact; a node dies and reads *and writes* keep committing on the survivors; it rejoins, catches up,
and must **agree** rather than merely answer, including on the row written while it was down; and
the full suite is re-run on the recovered node. **A rolling version upgrade is deliberately not
implemented** — it needs a second image digest *and* a second from-source artifact build against
that image's headers — so G5 stays partially open, stated here and in `yb/RUNBOOK.md §5` rather
than quietly dropped.

**`just bench-yb`** (G7) — insert, scan, point-lookup, aggregate, in-backend arithmetic and on-disk
size for `kmoney` vs `text` vs `numeric(36,18)` **plus its companion currency column**, because
comparing a self-describing 18-byte value against a bare `numeric` prices only half the schema.
**No pass/fail threshold, deliberately:** a first measurement has nothing to regress against, and a
limit invented before there is one either never fires or fires on somebody else's hardware.

Measured 2026-07-25, 20 000 rows per table, 3-node RF=3 cluster on one 28-core host. **Read these
as ratios, not absolutes** — the absolute numbers say more about that host than about YugabyteDB,
and two runs of this same recipe varied by 10–15 % on every timed row while the ratios held:

| operation | `kmoney` | `text` | `numeric(36,18)` + `char(3)` |
|---|---:|---:|---:|
| `INSERT … SELECT` 20 000 rows | 315 ms | 285 ms | 344 ms |
| full scan, projected as text | 36 ms | 21 ms | 64 ms |
| point lookup, 200 consecutive ids | 28 ms | 24 ms | 31 ms |
| 20 000 in-backend additions | **24 ms** | — | 55 ms |
| total over the whole table | **30 ms** (`kmoney_sum`) | — | 51 ms (`sum()`) |
| on-disk, 20 000 rows | **360 000 B** (18 B/row, exactly) | 608 891 B | 519 996 B |

**The palloc-per-result cost the readiness plan flagged as unmeasured is real, small, and visible
exactly where predicted**: the full scan materialises one 18-byte allocation per row and costs
~1.8× `text`, which simply hands back the bytes it stores. Everywhere the type does *work* rather
than return a string it wins — arithmetic and totalling are each roughly **half** `numeric`'s
cost, and the row is ~31% smaller than `numeric` once the currency column that type needs beside
it is counted. The aggregate row was **not** like-for-like when it was measured, and the report
said so: `numeric`'s `sum()` streams, while the `kmoney_sum(VARIADIC array_agg(...))` it was
compared against materialises an array first. `sum(kmoney)` now streams too (wide state, see §
above), so that caveat applies to the recorded numbers rather than to the type — re-measuring is a
task, not a correction.

The benchmark cross-checks `kmoney_sum` against `numeric`'s `sum()` and refuses to publish numbers
if they disagree — **through `kmoney`'s own parser, not string equality**. The two render
differently by design (canonical form versus padding to the declared scale), so the identical total
prints as `IDR 200019900.0000000000002` on one side and `200019900.000000000000200000` on the
other, and a naive comparison called that a disagreement on the first run. Feeding `numeric`'s
total back through `::kmoney` asks the question that actually matters — *is it the same money* —
using the one codec both paths already share.

**`just yb-soak`** (G7) — measured 2026-07-25: **5 604 transfers across 12 checkpoints, the total
unmoved at `IDR 120000.00` at every one of them, and no balance ever failing to re-parse to
itself.** The harness runs the concurrency load in a loop, asserting conservation **every round**
rather than once at the end, because "it broke somewhere in the last hour" is not a debuggable
fact. Logs land under `kamu-money-pg/yb/out/soak/`, not `/tmp`, which is tmpfs on this fleet.

**`release-check` carries every YB gate** — `yb-ab`, `test-yb-regress`, `test-yb-cluster`,
`test-yb-concurrent`, and (added after this entry was first written) `test-yb-readreplica` — and a
hygiene test asserts that composition, because dropping one for wall-clock is the obvious
temptation and the exact thing that would make the gate's claim untrue. It read *"all four"* here
for a commit after the fifth was added: the guard had been updated, the prose had not, and prose
that undercounts the gate is how a stage becomes droppable without anyone deciding to drop it.

### E20 — what `kmoney` actually costs, against `rust_decimal` and against `numeric` (measured 2026-07-26)

E19's numbers were a first baseline. This is the like-for-like comparison it said was owed, and it
answers a different question: **not "is it fast enough" but "where does the time go".**

Measured on PostgreSQL 18.4 (`kamu-money-pg:pg18`) and rustc 1.99.0-nightly, release profile,
500 000 rows / 1 000 000 iterations. The Rust fixture reports **best of N**; the SQL
fixture reports **bracketed medians** — each operation differenced against the mean of the
floor samples immediately before and after it, with the operation order rotated per pass.

> **Status: reproducible since 2026-07-26, and still not a threshold.** The original measurement
> code was written to answer a question and then discarded, which left this entry sitting under
> §1's rule — *"reproduce before trusting"* — as the one entry that could not be. E15 has
> `just yb-probe`, E17 has the case suite, E18 has the cluster script; E20 had nothing. It now has
> **`just bench-rust`** (`kamu-money-core/benches/kernel.rs`) and **`just bench-pg`**
> (`kamu-money-pg/bench/`), both of which retain raw samples, print the host they ran on, and
> assert correctness before timing anything.
>
> What has **not** changed is that there is no pass/fail limit, deliberately, and that neither
> fixture is in `check`, `check-all` or `release-check`. A timing that can fail a gate on a loaded
> machine is a gate that gets disabled. The figures below decide query shape, batching and where
> to put bulk arithmetic; they do not certify capacity, and a consuming service still owns its own
> end-to-end numbers.
>
> Two comparisons below are **deliberately asymmetric** and are not like-for-like: `kmoney` parse
> and render resolve and carry an ISO currency, which bare `numeric` never does. E19 measured the
> honest pairing — `numeric` plus a companion currency column — and that pairing, not the bare
> `numeric` figure, is the one a schema actually chooses between. `text::parse_amount` is the
> digits-only row, and it is the one to compare against `Decimal`.

#### Rust: `Money<C>` versus `rust_decimal::Decimal`

Values chosen inside **both** domains, because E4 established the two are incomparable at the edges
and a benchmark over `Money`'s full domain would be timing `rust_decimal`'s failure path.

| operation | `Money` | `Decimal` | ratio |
|---|---:|---:|---:|
| `checked_add` | **4.4 ns** | 9.3 ns | **0.47×** |
| sum of 1M, per element | 11.4 ns | 11.6 ns | 0.98× |
| `div_int` + take residue | 65 ns | 48 ns | 1.36× |
| parse, digits only | 86 ns | 20 ns | 4.2× |
| parse, with ISO code | 190 ns | — | not comparable |
| render | 318 ns | 92 ns | 3.5× |
| *floor:* raw `i128::checked_add` | 1.5 ns | — | — |

**The money kernel is faster than the alternative.** A fixed scale means no scale reconciliation on
every operation, which is where `Decimal` spends its time — and E3 records that its reconciliation
is not merely slow but *lossy*. The domain check costs ~3× a raw `i128` add and is the whole reason
E2/E3's silent-corruption class cannot occur here.

Two rows flatter `Decimal` and should not be read as losses. `div_int` **forces the caller to
handle the residue** (C5); `Decimal::checked_div` discards it silently — the benchmark panicked on
its first run because it dropped the `Residue`, which is the drop-bomb doing its job. And
`text::parse` resolves an ISO currency that `Decimal` never has to.

#### PostgreSQL: `kmoney` versus `numeric(36,18)`

> ## RETRACTED AND REMEASURED, 2026-07-26
>
> **Every figure originally published in this sub-section was measured against a DEBUG build of
> the extension, and compared against `numeric` — which is PostgreSQL's own, release-built C.**
>
> `cargo pgrx install` defaults to the **debug** profile. `cargo pgrx package` defaults to
> release. The original run used the `kamu-money-pg:pg18` image, which the test matrix builds with
> `cargo pgrx test` — debug, and *correctly so*, because overflow checks and `debug_assertions` are
> what catch bugs. Reusing that image for a *timing* question applied a correctness argument to a
> performance one. The `.so` is ~64 MB debug against ~800 KB release; nobody looked.
>
> The isolated cost of that mistake, same host, same probe, serial:
>
> | | debug | release |
> |---|---:|---:|
> | a null `#[pg_extern]` (returns its argument) | 177 ns | **at the scan floor, unresolvable** |
> | `kmoney_hash` | 1 101 ns | **24 ns** |
>
> **45×.** The table below is the re-measurement, and it reverses the conclusion rather than
> softening it.
>
> **Production was never affected.** `kamu-money-pg/yb/Dockerfile` uses `cargo pgrx package`, so
> the shipped artifact has always been the release build — the 802 KB `.so` whose hash the release
> manifest records. What was wrong was every published number, and the design advice derived from
> them.

Per-row operations are reported with the **scan floor subtracted** (a bare predicate over the same
500 000 rows, 22.94 ms on this run), because the interesting quantity is the type's own cost rather
than the table scan both types pay identically. Reproduce with `just bench-pg`.

| operation, 500k rows | `kmoney` | `numeric` | ratio | *originally published* |
|---|---:|---:|---:|---:|
| `a + b` | **14.4 ms** | 29.8 ms | **0.48×** | *6.4×* |
| `a - b` | **15.6 ms** | 26.3 ms | **0.59×** | *9.8×* |
| `sum(col)` | 23.8 ms | 17.0 ms | 1.4× | *~43×* |
| render `::text` *(asymmetric)* | 81.8 ms | 34.3 ms | 2.4× | *18×* |
| wire size, text | **6.3 MB** | 12 MB | 0.52× | unchanged |

**`kmoney` is about twice as fast as `numeric` at arithmetic**, which is what the Rust figures
above predicted all along and what the debug build hid: a fixed scale needs no scale
reconciliation per operation, and that is where `numeric` spends its time. The `sum` row is 1.4×
rather than ~43×, and render — the one row where `kmoney` is genuinely slower — is 2.4× rather than
18×, for a form that carries an ISO currency `numeric` does not have.

The `COPY` rows are not restated because they were wall-clock without a floor and have not been
re-measured; treat them as withdrawn rather than corrected.

### The pgrx call boundary is not the cost. RETRACTED CLAIM, and the isolation that killed it

This sub-section previously read *"A pgrx call in this extension costs ~6–12× a native C call, and
that single fact explains every row above."* **It is withdrawn.** It was the debug build.

An external review had already objected that the pair it rested on — `abs(numeric)` against
`kmoney_hash` — is not doing the same work, so it bounds the boundary from above rather than
isolating it, and named the missing experiment: *a null `#[pg_extern]` against a null C function*.
Running it produced something the review did not predict and neither did I.

The probe is two functions with **identical signatures**, `bigint → bigint`, each returning its
argument: `c_noop` compiled from five lines of C against the server headers, and `rs_noop` as a
`#[pg_extern]`. Plus `rs_noop_kmoney`, which takes the 18-byte type and does nothing with it, so
the conversion of *our* type separates from any function body. Serial
(`max_parallel_workers_per_gather = 0`, `EXPLAIN` confirming no `Gather` — the first attempt planned
two workers and reported wall-clock-per-row as though it were CPU-per-call). Floor subtracted:

| | debug build | **release build** |
|---|---:|---:|
| native C, `c_noop(bigint)` | 2.5 ns | 0.6 ns |
| pgrx, `rs_noop(bigint)` | 177 ns | **at the floor, unresolvable** |
| pgrx, `rs_noop_kmoney(kmoney)` | 319 ns | **at the floor, unresolvable** |
| pgrx, `kmoney_hash(kmoney)` | 1 101 ns | **24 ns** |

**In release, a pgrx call cost less than this method could resolve.** Both null pgrx functions
landed *below* a floor sample, i.e. under the pass-interleaved sampler's ~5 ns noise. That is a
statement about the ruler, not about the cost — the bracketed sampler that replaced it resolves
`kmoney_hash` at 27 ns/row where this one could not separate it from the floor at all, and the
YugabyteDB probe below, with a stable floor, resolves the null-function difference at ~4 ns. The
null probes have **not** been re-run under the bracketed method. And in the widened
comparison, `kmoney_hash` at 17 ns/call sits **between** two native C functions — `hashint8` at
10 ns and `abs(numeric)` at 37 ns. There is no pgrx band above a native band. There never was one;
there was an optimised binary being compared against an unoptimised one.

What the debug run had actually decomposed, for the record, was: fmgr dispatch 2.5 ns, pgrx's
unoptimised wrapper 177 ns, unoptimised `FromDatum` for the 18-byte type 140 ns, and the
unoptimised FNV+`fmix64` body 782 ns. Every one of those is an artefact of `-C opt-level=0`.

**And on YugabyteDB, which is the number that decides anything here.** Re-run from the tracked
fixture — `just bench-boundary-yb`, which builds `--target boundary-node` so both functions are
compiled against YB's own PG15 headers and glibc and load into the same backend (2026-07-26, host
load 8.58, floor spread **1.06**, 9 passes, bracketed and rotated, serial asserted, elimination
check passed):

| | median ns/call | passes below the floor |
|---|---:|---:|
| null C, `c_noop(bigint)` | **7.84 ns** | 0 of 9 |
| null pgrx, `rs_noop(bigint)` | **11.29 ns** | 0 of 9 |

**~3.5 ns of pgrx wrapper on YugabyteDB**, against ~6.8 ns on stock PG18 — single-digit on both,
and that is the whole finding. **Do not read the difference between the two engines as a result.**
The runs were minutes apart on a host whose load moved from 3.91 to 8.58, the `c_noop` baselines
differ by nearly 2× between them, and the per-pass `c_noop` deltas on the YB run spread 2.9–7.5 ns
— wider than the wrapper being measured. What each run establishes is the *within-run* paired
difference on its own engine; comparing across runs is the sequential-sampling error one level up.

The earlier one-off measurement of this same pair — 2.1 ns and 6.0 ns, pass-interleaved, floor
spread 1.10 — is superseded but **agreed**: ~4 ns then, ~3.5 ns now, by a different sampling
method on a different day from a fixture that did not exist then. That agreement is worth more
than either figure.

This matters because YSQL is multi-threaded, so
`CurrentMemoryContext` is the thread-local `YbCurrentMemoryContext` that pgrx's wrapper touches on
every call: a TLS access costs more than a process-global load, and that was the one *named*
mechanism for the boundary to be dearer on the deployment target than on stock PostgreSQL. It is
not: the wrapper is single-digit nanoseconds on YugabyteDB, measured on YugabyteDB. Measuring it
on PG18 alone would have been §0.4's own error — *"it passes on PostgreSQL" is not evidence about
YugabyteDB* — so both were run, and both are now reproducible from tracked fixtures.

The YB probe deliberately used `generate_series` rather than a table. DocDB is then out of the
path while the thread-local memory context is unchanged, which is what makes a 4 ns difference
resolvable at all: over a real table YugabyteDB's scan floor is ~378 ms against stock
PostgreSQL's ~23 ms, and its *variance* — not its magnitude — swamps everything. A large floor
cancels on subtraction; an unstable one does not.

**The probe is now tracked, and it was not** (corrected 2026-07-26 after a review found this
entry unreproducible). `c_noop` in C, and `rs_noop`/`rs_noop_kmoney` as `#[pg_extern]`s, were
appended to `lib.rs` inside a container from a `git archive` of the commit under test and
committed nowhere -- so the sentence that used to sit here, *"reproducing it is a container, not an
archaeology exercise"*, was false: neither probe source, build script, recipe nor raw output
existed in any revision. A figure that steers architecture has to be re-derivable by someone who
was not there. It now lives at `kamu-money-pg/bench/boundary/` behind
`--features boundary-probe`, run by **`just bench-boundary`**, still absent from the shipped SQL
surface.

Re-run from the tracked fixture on stock PG18 (2026-07-26, host load 3.91, floor spread **1.03**,
9 passes, bracketed and rotated, serial asserted):

| | median ns/call | passes below the floor |
|---|---:|---:|
| null C, `c_noop(bigint)` | **4.27 ns** | 0 of 9 |
| null pgrx, `rs_noop(bigint)` | **11.09 ns** | 0 of 9 |

**~6.8 ns of pgrx wrapper on stock PG18**, against ~4 ns on YugabyteDB. This is the first time the
stock-PG18 wrapper has been *resolved* rather than declared unresolvable: `generate_series` has no
I/O, so its floor holds to 1.03 even on a loaded host, and the bracketed sampler resolves what the
pass-interleaved one could not. The two platforms agree to within a few nanoseconds, which is the
point — the multi-threaded YSQL thread-local memory context was the one named mechanism for the
boundary to be dearer on the deployment target, and it is not.

**The `kmoney`-typed rows are deliberately absent from that table, and the reason is a benchmark
that measured nothing — this file's ninth.** The tracked probe's first version measured
`rs_noop_kmoney` and `kmoney_hash` with a *constant* argument, `'USD 1.25'::kmoney`, specifically
to avoid invalid-benchmark #1 (a predicate that builds its own argument and times the parser). It
avoided that and walked into #2: both functions are `IMMUTABLE` and the argument was constant, so
the planner **constant-folded the call**. It ran once at plan time and never per row, and both
rows measured 26–28 ns/call *below* the floor in 9 passes of 9 — faster than doing nothing, which
is the signature of an eliminated expression. The two forms are a vice: an argument that varies
with the row has to be *built*, which costs hundreds of nanoseconds; one that does not vary gets
folded away. There is no third option without a per-row source of `kmoney` values, which means a
table, which puts DocDB back in the path on YugabyteDB. So the boundary is measured with the
`bigint` pair, which answers the question the pgrx argument actually rests on, and `sql-cost.sql`
prices `kmoney_hash` over a real table where a scan is the point rather than the obstacle.

The probe now **refuses** a row whose median sits at or below the floor, naming it as elimination
rather than speed. That check is what caught the fold.

`sum()` still decomposes, and the surviving question is smaller than it looked: `sum(kmoney)` costs
23.8 ms floor-subtracted against `sum(numeric)`'s 17.0 ms, so R2-F4b's `bytea`-over-`internal`
choice costs **~1.4× on column totals**, not the ~10× this entry previously bounded it at. Whether
that residue is varlena copying remains untested — the `internal`-state variant has still not been
built — but the prize for building it has shrunk by an order of magnitude, and the decision to use
`bytea` is now comfortable rather than merely defensible.

The consequence for callers changes accordingly. *"Prefer one coarse call to many fine ones"* was
advice for a 380 ns toll that does not exist; per-call overhead is not a reason to restructure a
query. What survives on its own evidence: bulk arithmetic in Rust is still faster than in SQL
(E20's Rust table, and `Money` beats `rust_decimal`), binary ingest is still smaller on the wire,
and `kmoney` is now *faster* than `numeric` at the arithmetic itself.

#### Storage, apples to apples — and the one row where `kmoney` loses

Measured 2026-07-26 on PostgreSQL 18.4, with `pg_column_size`. **No timing, no floor, no quiet
host required** — this is a deterministic property of the two representations, and it was being
treated as a benchmark result when it is a fact.

`numeric` is variable-length; `kmoney` is fixed at 18 bytes. And a `numeric` money column is not
a money column until a currency sits beside it, so the honest pairing carries `char(3)` (4 bytes):

| value | `kmoney` | `numeric+cur` | `numeric` bare | Δ vs `numeric+cur` |
|---|---:|---:|---:|---:|
| `0.00` | 18 | 10 | 6 | **+8** |
| `12.34` — typical retail | 18 | 14 | 10 | **+4** |
| `99999.99` — typical ledger | 18 | 16 | 12 | **+2** |
| `999999999.99` — large ledger | 18 | 18 | 14 | **0** |
| `1.123456789012345678` — after an FX rate is applied | 18 | 22 | 18 | **−4** |
| `999999999999999999.999999999999999999` — domain max | 18 | 30 | 26 | **−12** |

**`kmoney` costs storage for values that never use the precision, and saves it for the values
this library exists to hold exactly.** On PostgreSQL the crossover is around `999999999.99` —
roughly eleven significant digits — so a retail ledger of two-decimal amounts pays about
**+4 bytes per row**, a table of post-conversion amounts carrying 18 decimals *saves* four, and
one at the domain top saves twelve.

**On YugabyteDB the crossover arrives far earlier, and the table above understates `kmoney`.**
`char(3)` costs 7 bytes there against 4 on PostgreSQL, so the companion column a `numeric` money
schema needs is nearly twice the price. Measured from stored rows on the deployable node image,
2026-07-26:

| value | `kmoney` | `numeric+cur` | Δ on YB | Δ on PG18 |
|---|---:|---:|---:|---:|
| `0.00` | 18 | 13 | +5 | +8 |
| `12.34` — typical retail | 18 | 17 | **+1** | +4 |
| `99999.99` — typical ledger | 18 | 19 | **−1** | +2 |
| `999999999.99` — large ledger | 18 | 21 | −3 | 0 |
| `1.123456789012345678` — after an FX rate | 18 | 25 | −7 | −4 |
| domain max | 18 | 33 | −15 | −12 |

So on the engine this actually deploys to, `kmoney` costs **one byte** a row at retail amounts and
is already *cheaper* by a typical ledger figure. The PostgreSQL penalty is the pessimistic case,
not the operative one.

That correction exists because the first attempt at this table added a hardcoded 4 bytes for the
currency column — PostgreSQL's `char(3)` size — and ran it against YugabyteDB, which made one
engine's answer wear the other's. The figures above are `pg_column_size` over stored rows on each
engine.

That +4 is a real cost and it is stated here rather than left for someone to discover: `kmoney`'s
fixed width is the same property that makes it 18 bytes with no varlena header, no TOAST
decision, and no per-row length to read — and the same property that makes it larger than a
`numeric` holding `12.34`. The trade is bounded and it does not move with the row count in any
surprising way, because it does not depend on the data at all, only on its shape.

It also does not change the design. §0.1's axiom is that truth is stored and the residue cannot
be silently dropped, which is a correctness requirement; four bytes a row is a price, and the two
are not commensurable. Recorded because a design record that only lists the comparisons its
subject wins is an advertisement.

#### On YugabyteDB, which is where this deploys

Measured 2026-07-26, `just bench-sql-yb 100000 9`: the deployable node image, 100 000 rows,
9 passes, bracketed and rotated, serial plans asserted, bracket drift median 7.50% (the gate),
global floor spread 2.15 (context). Fewer rows than the PostgreSQL fixture because DocDB writes
cost more; the per-row figures normalise.

**Read `median_ns_per_row` together with `noise_ns_per_row`.** A median at or below its own noise
is not a measurement of that row, and on YugabyteDB that is the common case for `kmoney` rather
than the exception — which is itself the result.

| operation | ns/row | noise | resolved? |
|---|---:|---:|---|
| `numeric+cur`: `a + b` (currency checked) | **1081.6** | 137.7 | yes, 7.9× |
| `numeric+cur`: `a - b` (currency checked) | **1060.7** | 88.2 | yes, 12× |
| `numeric`: `a - b` (bare) | **790.7** | 144.0 | yes |
| `numeric`: `a + b` (bare) | **771.4** | 127.7 | yes |
| `numeric+cur`: parse from stored text | **720.2** | 87.0 | yes, 8.3× |
| native C: `abs(numeric)` | **686.6** | 118.2 | yes |
| native C: `n = n` | **530.4** | 132.2 | yes, 4× |
| `numeric+cur`: render canonical | **359.9** | 110.0 | yes, 3.3× |
| `numeric`: render `::text` (bare) | **201.7** | 67.8 | yes, 3× |
| `kmoney`: parse from stored text | 185.6 | 124.4 | marginal |
| native C: `hashint8(id)` | 167.2 | 88.0 | marginal |
| `sum(kmoney)` | 134.6 | 131.1 | **no** |
| `pgrx`: `kmoney_hash(m)` | 132.3 | 75.7 | marginal |
| `kmoney`: render canonical | 111.7 | 115.3 | **no** |
| `kmoney`: `a - b` | 85.2 | 82.5 | **no** |
| `kmoney`: `a + b` | 28.0 | 95.4 | **no** |

**Three things this supports, and one it does not.**

1. **`kmoney` arithmetic is at least ~8.8× cheaper than `numeric+cur` on YugabyteDB, as a LOWER
   BOUND.** `numeric+cur: a + b` is 1081.6 ± 137.7 and resolved; `kmoney: a + b` is somewhere
   under ~123 ns/row (median plus its own noise). The ratio cannot be stated more precisely than
   that, because only one side of it was measured. Three separate runs agree on the shape.

2. **The currency discipline costs ~310 ns/row in `numeric`, and nothing in `kmoney`.** Bare
   `numeric: a + b` is 771.4 and currency-checked `numeric+cur: a + b` is 1081.6 — both resolved,
   so the difference is real. `kmoney` performs that same check inside the operator, on every add
   already counted in its own row. The bare figure is what a schema pays only if it is willing to
   add USD to JPY.

3. **The dominant per-row cost on YugabyteDB is paid before any arithmetic runs — and it is
   SPECIFIC TO `numeric`, not to variable-length columns.** Plain `native C: n = n`, doing no
   money work at all, costs 530.4 ns/row while the same shape over a `bigint` costs 167.2.

   **This entry first attributed that to varlena-versus-fixed-width decoding, and that was
   wrong.** It is a generic story — it would hold on stock PostgreSQL too — and it does not
   explain the gap *widening* from ~2.7× on PG18 to ≥8.8× here. The operator proposed the
   mechanism that does: YugabyteDB separates storage from compute, DocDB models decimals
   natively, so a PG `numeric` is translated at the boundary on every read while a type DocDB
   has never heard of can only be handed back as opaque bytes.

   `bytea` decides between the two, being variable-length like `numeric` and opaque like
   `kmoney`. Measured by the tracked fixture (`just bench-why-yb`, 100k rows, 9 passes, bracket
   drift 8.61%), all five columns holding the same value:

   | column | per-row bytes | ns/row | noise |
   |---|---:|---:|---:|
   | `bigint`, `i = i` | 4 | −105.3 | 99.8 |
   | `text`, `t = t` (**varlena**, YB-native) | 11 | 54.3 | 40.4 |
   | `bytea`, `b = b` (**varlena**, **opaque**) | 22 | 71.5 | 111.2 |
   | `kmoney`, `m = m` (fixed 18 B, **opaque**) | 18 | 98.5 | 58.8 |
   | `numeric`, `n = n` (**varlena**, YB-native) | 10 | **586.0** | 110.1 |

   `bytea` and `numeric` are both varlenas and `bytea` lands with `kmoney` and `text`, six to
   eleven times below `numeric`. **Variable length is not the cost**, and `text` being cheap
   rules out "YB-native types are dear" as well. Note the byte counts: `bytea` is the WIDEST
   column here at 22 bytes and among the cheapest, while `numeric` is the NARROWEST varlena at
   10 and by far the dearest. Width does not order this table; representation does.

   **Only the `numeric` row is solidly resolved** (5.3× its own noise). The other four sit at or
   under their own noise and are a BAND, not figures — an earlier independent run of the same
   probe put them in a different internal order (text 97.1, kmoney 109.3, bytea 133.6) with
   `numeric` at 550.7, which is the same conclusion and confirms that their ordering was never
   meaningful. What carries the argument is one row sitting far outside the band, twice.

   **Shown versus inferred:** shown, that the cost is numeric-specific and unexplained by
   variable length. Inferred, that DocDB's own decimal representation is the mechanism — that
   follows from YugabyteDB's architecture and is not measured here; reading DocDB's encoder
   would settle it, and that is outside this repository.

   **The figures above are not independently recomputable from their own transcript, and the
   conclusion does not rest on them.** A 2026-07-26 audit found that
   `why-numeric-costs.sql` retained only its median table: no position column, no raw samples, no
   plan assertion, and no check that the five predicates matched the same rows. The fixture now
   records all four, so the *next* run restates this table from evidence a reader can recheck.
   What survives that gap unaffected is what carries the argument — the per-row byte counts, which
   are printed, and the ordering of the rows, which two independent runs agree on.

   It also sharpens §0's *"NEVER use NUMERIC"* ruling, which until now rested entirely on
   correctness (E9's silent rounding, E13's rounding on the way in). On the deployment target it
   is the most expensive column type measured here as well.

4. **NOT supported: any per-row figure for `kmoney` itself on this engine.** Its operations are
   below the noise the storage layer generates, across three runs, pinned and unpinned. That is a
   statement about the ruler. If a `kmoney` per-row cost is ever needed it has to come from the
   PostgreSQL fixture, where the floor is an order of magnitude smaller, or from a method that
   takes DocDB out of the path the way the boundary probe does.

#### Nine benchmarks that measured nothing, and the rules that catch them

Recorded because the failure is systematic and the corrected numbers above only exist because each
one was eventually caught. Every one produced a confident, wrong answer that survived review:

| what was written | what it concluded | what was actually happening |
|---|---|---|
| `('USD '\|\|g\|\|'.25')::kmoney` over `generate_series` | parse is at parity with `numeric` | string concatenation and series generation swamped both sides |
| `count(*) FROM (SELECT m + m …)` | arithmetic is at parity | `count(*)` does not need the column; the projection was **eliminated and never ran** |
| `count(*) FROM (SELECT kmoney_sum_accum(…) …)` | the transition function is free, so the cost is state plumbing | same elimination — this one produced a *root cause* that was pure artefact |
| **(2026-07-26)** `noop_sum` / `cnt_sum`: aggregates with the same `bytea` state doing **no arithmetic**, and a pass-by-value state copying no varlena | the difference brackets what the varlena state handling costs | both came back **slower than the real aggregate** — 661 ms and 709 ms against `sum(kmoney)`'s 47 ms. They are `LANGUAGE sql` functions, and a SQL-language transition function costs more per row than the pgrx one it was meant to be a cheaper baseline for. They measure SQL call overhead, not state plumbing |
| **(2026-07-26)** every SQL figure in this entry, from its first publication until the retraction above | `kmoney` is 6–43× `numeric`, and a pgrx call costs ~376 ns | **the extension was a DEBUG build.** `cargo pgrx install` defaults to debug; the image came from the test matrix, which builds debug *correctly*, for tests. Compared against `numeric` — PostgreSQL's own release-built C. The truth is the reverse: `kmoney` is ~2× **faster** at arithmetic |
| **(2026-07-26)** the first YugabyteDB boundary run, sampled sequentially — all reps of one query, then the next | a null **C** function is *slower* than a null pgrx function | the rows were never compared to each other. YB's floor drifted 2.4× across the run, so queries measured minutes apart under different conditions were differenced as though the floor were constant. Stock PostgreSQL's floor is stable enough that the same harness looked fine there for months. Fixed by paired interleaved passes; the corrected run put the floor spread at 1.10 |
| **(2026-07-26)** the *second* YugabyteDB run, on the two `kmoney` rows | converting the 18-byte type costs ~414 ns/row | the predicate built its own argument — `('USD '\|\|(g%100)\|\|'.25')::kmoney` — so it timed string concatenation and parsing. **This is invalid-benchmark #1 in this very table**, reproduced by its author while measuring the subject of the table. The two `bigint` rows were unaffected and are the ones quoted above |
| **(2026-07-26)** the pass-interleaved sampler that replaced sequential sampling — floor once per pass, every operation at a fixed offset behind it | drift cancels within a pass, so the deltas are paired | the floor ran FIRST and each operation kept the SAME position on every pass, with the two ~620 ms `LANGUAGE sql` controls always last. So the amount of preceding work between an operation and the floor it was differenced against was a constant property of that operation, not something that cancelled. Three rows sat below the floor in 8-9 passes of 9 and no figure could be read from them. Fixed by measuring a floor BETWEEN every pair of operations and rotating the order per pass |
| **(2026-07-26)** the TRACKED boundary probe's first version, on its two `kmoney` rows: argument pre-built as the constant `'USD 1.25'::kmoney` | the per-row cost is the boundary alone, with the parser out of the path | the functions are `IMMUTABLE` and the argument was constant, so the planner **constant-folded the call**: it ran once at plan time and never per row. Both rows measured 26-28 ns/call *below* the floor in 9 passes of 9 — faster than doing nothing. Written while avoiding #1 in this table and landing on #2, in the file whose header lists both. The `bigint` rows were unaffected. Fixed by measuring the boundary with the `bigint` pair only, and by making the probe REFUSE any row whose median sits at or below the floor |

The tell was in the output the whole time: `count(*)` floor 15.40 ms, add 15.93, subtract 15.05,
render 15.63. Four different operations within 0.5 ms of the floor is not parity.

> **A benchmark result equal to the floor is elimination until proven otherwise.** Force evaluation
> with a predicate on the computed value, and always measure the floor alongside so the comparison
> is available to be made.
>
> And its companion, learned the harder way on the fourth attempt: **a control is only a baseline
> if it is cheaper than the thing it is a baseline for.** Check that before reading anything into
> the difference — `noop_sum` was built to do strictly less work than `sum(kmoney)` and took 14×
> as long, because "less work" was measured in the wrong currency.
>
> And the fifth, which cost more than the other four together: **measure the profile you ship, and
> make the harness refuse the one you do not.** `cargo pgrx install` builds debug and
> `cargo pgrx package` builds release, so "the same artifact the tests use" — which sounds like
> rigour, and is the right rule for correctness — silently means "unoptimised" for a timing
> question. The tell was a 64 MB `.so` where the shipped one is 800 KB, visible on any `ls` for
> months. `bench-pg` now passes `--release` and **refuses to run against an artifact over 8 MB**,
> because the previous version of this rule was a comment nobody read.

This is [[correctness-must-be-reproducible]] applied to measurement: a number that cannot be
reproduced *as a measurement of the thing named* is not evidence, and three of these passed every
sanity check except the one that mattered.

#### One design decision, confirmed by an error message

`EXCEPT ALL` over a `kmoney` column fails with `could not identify an equality operator for type
kmoney`. That is R2-F5 working: the type deliberately carries no B-tree or hash operator class, so
set operations, `DISTINCT` and `GROUP BY` on an amount are refused at plan time rather than
silently answering a question about money that nobody should be asking of it. The binary round trip
was verified on the text form instead — 500 000 values out and back, zero mismatches.

---

## 2. Contracts

### C1 — Canonical representation

```rust
pub struct Money<C: StaticCurrency> { units: i128, _c: PhantomData<C> }   // exactly an i128
```

- **Invariant:** `units` is a count of `1e-18` currency units. Scale is **fixed at 18, structurally** — it is not a field, so it cannot drift. `|units| < 10^36`.
- **Invariant: one scale for every fixed-point type in the crate**, money and rates alike. Not for arithmetic reasons — money's scale *cancels* in `Money × Rate` (the result is `m*r / 10^(rate scale)`, so the divisor is the rate's scale alone and the formula is identical at 12 or 18). The reason is that a schema holding both `numeric(36,12)` and `numeric(36,18)` requires a human to remember which column is which, where a mistake is a **silent factor of `10^6`** and no type system reaches a migration, an ad-hoc query, or a BI tool. One scale makes that error unrepresentable rather than documented — the same move that deleted `StaticCurrency::EXP`.
- **Note:** `|units| < 10^36` does **not** move with the scale, because it counts units. The `i128` checking margin is ~170x at 12 and ~170x at 18, identically. Widening the scale cost integer range (`|v| < 10^18` rather than `< 10^24`), not headroom.
- **Invariant:** the raw `i128` is never publicly reachable. A caller holding one could reintroduce an unchecked construction path.
- **Failure:** construction outside the domain → `Err(DomainOverflow)`. Never truncation, never saturation.
- **Evidence:** E3 (Decimal cannot hold this invariant), E4, E5, E6.

**Why `i128` and not `Decimal`:** the schema *defines what money is*. `NUMERIC(36,18)` declares money to be a multiple of `1e-18` up to `10^18`. `i128@18` represents that set exactly and totally; `Decimal` represents 0.000008% of it (E5) and cannot survive addition within it (E3). That percentage is unchanged by the scale: both sides of it are counts of **units**, and the domain is `10^36` units at any scale.

**Why the 170× headroom is not waste:** it is the **checking margin**. Two in-domain values sum to at most `2e36`, which is 85× below `i128::MAX`, so the domain check always runs *after* the arithmetic. No pre-check, no wrapping, no UB. The domain bound and the machine bound being far apart is what makes the invariant cheap.

### C2 — Currency identity

```rust
iso4217! {
    USD = 840, exp = 2,  "US Dollar";
    IDR = 360, exp = 2,  "Rupiah";
    JPY = 392, exp = 0,  "Yen";
    KWD = 414, exp = 3,  "Kuwaiti Dinar";
    XAU = 959, exp = NA, "Gold";
    // ... ~180 entries
}
```

- **Invariant:** this table is the **sole** source of truth. It generates the `#[repr(u16)] Iso4217` enum, one ZST per currency, and every `const fn` lookup. `Usd::CODE` and `Iso4217::USD` cannot drift because one table wrote both.
- **Invariant:** the set is **closed**. An unrecognized code is a parse error, never a silent pass.
- **Invariant: the table is GENERATED, never authored.** All 178 codes of ISO 4217 as published 2026-01-01, expanded at COMPILE TIME by the `kamu-money-iso` proc macro from the maintenance agency's `list-one.xml`, vendored with its checksum and credit at `kamu-money-iso/`. There is no generated file and no verifier: the register and the table are the same object, and its invariants are checked as it is read, so a bad edition fails the build of every crate downstream rather than a test someone can skip. This is not tidiness: of the 178 codes, 17 take **0** fraction digits, 139 take **2**, 7 take **3**, 2 take **4**, and 13 have **none** — a hand-typed table would review as correct and settle amounts wrongly, which is this crate's own failure mode reached through its reference data. The twelve hand-written entries it replaced were checked against the source first; all twelve matched, which is why the pipeline is trusted rather than merely convenient.
- **Consequence, found on completion:** `Iso4217::ALL` had to become `Iso4217::EVERY`. `ALL` is the Albanian lek, so the associated const listing every currency was shadowed by its own variant the moment the register was complete — surfacing as `` `Iso4217` is not an iterator ``, a confusing way to be told a constant was overwritten by data. An associated item sharing a namespace with externally-defined identifiers will eventually collide, because the register grows and its names are not ours to choose.
- **Invariant:** `exp` is the **ISO settlement exponent** — the standard's number, not the market's. IDR is `2` per ISO 4217, even though sen are extinct in practice.
- **Invariant:** `exponent()` returns `Option<u8>`. `XAU`/`XDR`/`XXX` have no exponent. Gold has no cents.
- **Failure:** display dp ≠ settlement dp (IDR renders 0dp, settles 2dp). Display dp lives in `LocalePolicy` and **never touches the wire**. Two numbers, two homes, no drift.
- **Built (2026-07-22), and the failure it actually deletes is sharper than the one anticipated.** The hazard was never "IDR looks wrong at 2dp" — it is that a display width *below* the settlement width invites the renderer to truncate. `Money<IDR>` holding `16000.50` under a 0dp policy must render `Rp 16.000,5`, never `Rp 16.000`: the second drops five hundred rupiah to a formatting decision and is §0.1's second number wearing its last available costume. **Measured, not asserted:** inserting `fraction.truncate(min_fraction_digits)` into the renderer fails **7 of `tests/locale.rs`'s 13 tests**, both property tests among them. A prediction that only the dedicated test would catch it was written first and was wrong.

### C3 — The currency IS the type

```rust
pub trait StaticCurrency: private::Sealed { const CODE: Iso4217; }   // sealed, macro-implemented
pub struct Money<C: StaticCurrency> { units: i128, _c: PhantomData<C> }
```

- **Invariant: there is no runtime-currency variant.** A `Money<C>` is 16 bytes — exactly an
  `i128` — because `C` is a ZST and the currency is carried entirely by the type. `Money<USD> +
  Money<IDR>` is a compile error with no runtime check to forget and no error arm to handle.
- **Invariant: the trait is sealed.** `private::Sealed` is unnameable downstream, so no external
  crate can mint a counterfeit currency claiming `CODE = Iso4217::USD`. A doc comment saying
  "implemented by the macro, never by hand" was **not** access control — verified: a counterfeit
  compiled and impersonated genuine USD before the seal existed.
- **Where a runtime currency is genuinely needed, the SCHEMA declares it, not Rust.** A money
  column is either single-currency (`kmoney(IDR)`, typmod-pinned) or mixed (`kmoney_mixed`),
  in its DDL — so the decode target follows from the column type. See C8.

> **Correction (2026-07-22), and it deletes most of this contract.** C3 previously specified a
> *unified static/dynamic type*: a `CurrencyRepr` trait selecting a `Tag` of `()` or `Iso4217`, a
> blanket impl, a `Dyn` marker, `erase`/`try_cast`, and `try_add`/`try_sub` on `Money<Dyn>`. All
> of it is gone — 327 lines.
>
> The objection that landed was not "the dynamic variant is unnecessary" but something sharper:
> **`Money<Dyn>` offered arithmetic.** It looked like money and had `try_add`/`try_sub`, so it
> invited callers to *compute* in the unchecked mode. C4 had already removed `impl Add` from it
> to discourage exactly that — which is the design conceding the type was a hazard while keeping
> it. A boundary is a place you pass through, not a place you work.
>
> Nothing replaces it yet, deliberately. The decoder that would need a boundary form does not
> exist until phase 4, and a fallible surface no caller can reach is the same speculative
> complexity that got `from_parts_unchecked` deleted (§0.3). `MoneyError` is `#[non_exhaustive]`,
> so re-adding its "expected `C`, found `X`" arm costs nothing when there is a caller.
>
> The earlier E0119 correction under this contract stands as history and is preserved in E11: it
> recorded that `impl<C: StaticCurrency> CurrencyRepr for C` alongside `impl CurrencyRepr for Dyn`
> **does** compile, against an assertion made from memory that it did not. Both impls are now
> deleted, so the finding no longer binds this crate — but the lesson it was recorded for does.

### C4 — Arithmetic

| Operation | Contract |
|---|---|
| `Add`/`Sub`/`Neg` for `Money<C: StaticCurrency>` | **Exact. Rounding is structurally unrepresentable.** Fallible only on domain overflow, loudly (panic on `+`/`-`, `checked_*` where handled). |
| `Money::try_sum` for `Money<C>` — **not `Sum`** | Wide-accumulated (`I256`), one domain check at the end, returns `Result`. `Sum` was removed (R2-F4): a fold through `+` fails on a transient partial sum, so it was order-dependent. `try_sum` is a function of the values, not their order. It stays removed even though SQL's aggregate came back (R2-F4b) — a SQL aggregate can be given a wider state than its element type, and a `Sum` fold through `+` cannot. |
| ~~`Money<Dyn>`~~ | **Deleted with the type** (C3). `try_add`/`try_sub` existed because the currency could disagree at runtime; it no longer can. The reasoning that produced them — *"a `+` that can fail is a lie"* — is what eventually deleted the type itself: a `try_+` that can fail is the same lie with an escape hatch. |
| `mul_rate`, `div_int` | Widen to `ethnum::I256`. Take an explicit `Rounding`. Return a **`Division`**, never a tuple — see C5. |
| `allocate(&[w])`, `split(n)` | **`sum(parts) == self`, exactly. Always.** |

- **Invariant:** `i128 * i128` overflows `i128` (E7), so every multiply widens to `I256` and narrows with an explicit mode. There is no unwidened multiply path.
- **Failure:** `i128::MIN.checked_neg()` → `None` (E7). Two's-complement asymmetry is real and must be handled, not assumed away.
- **Evidence:** E7.

**Correction on the record:** "Add is exact and total" was claimed during design and is **false** (E7). Add is *exact* — it never rounds — but it is *not total*; it can overflow. The property that matters is **detectability**: `i128` overflow is `None`; `Decimal` scale-drop is `Some` (E3). Loud versus silent, not total versus partial.

### C5 — Residue: the "loses no money" mechanism

```rust
#[must_use = "a Division holds money. Decide the residue: .take_residue() or .discard_deliberately()."]
pub struct Division<C> { quotient: i128, residue: i128 }   // private fields, no public accessor

impl<C: StaticCurrency> Division<C> {
    fn take_residue(self) -> (Money<C>, Residue<C>);       // opt IN to holding the obligation
    fn discard_deliberately(self) -> Money<C>;             // no Residue is ever constructed
}

#[must_use = "this residue is MONEY. absorb it: .take_units() and post it, add it back, or .discard_deliberately()."]
pub struct Residue<C> { units: i128, ack: bool }
```

- **Invariant: a lossy operation returns ONE value, never a tuple.** `div_int` yields a `Division`,
  which bundles the quotient and the residue so they cannot be separated. There is no way to reach
  the money without choosing what happens to the residue, and therefore **dropping an undecided
  `Division` is safe**: nothing was handed out, so nothing left the ledger.

  | Caller writes | Old tuple API | Now |
  |---|---|---|
  | `let (share, _) = m.div_int(..)` | nothing warns; drop-bomb at runtime | **does not compile** — there is no tuple |
  | `m.div_int(3, HalfEven);` | `#[must_use]` warns | `#[must_use]` warns |
  | `Division` dropped mid-unwind | silent loss (see the hole below) | nothing was produced |
  | `let (share, _) = div.take_residue();` | — | **nothing warns.** The drop-bomb, at runtime |

- **Correction (2026-07-22), and it invalidates this contract's central claim.** Three revisions of
  C5 argued about `#[must_use]`, drop-bombs, panic-versus-count, and unwind safety. Every one of
  them was **downstream of a signature that did not need to exist**: `-> (Money<C>, Residue<C>)`.
  A tuple hands the caller two independent values, so one can be kept and the other dropped, and
  each guard was policing that separation rather than removing it. C5 concluded *"a `Drop` impl
  cannot be forbidden, only made loud — this is the strongest enforcement the language permits."*
  **That is false.** Bundling is stronger, and it is ordinary Rust. Verified by compilation: the
  quotient is unreachable except through a method that also decides the residue, and
  `let (_share, _) = m.div_int(..)` now fails with *"expected `Division<USD>`, found `(_, _)`"*.
  Pinned by `tests/ui/residue_wildcard_destructure`.

  **The general form is worth more than the fix:** if two values must be handled together, never
  return them as a tuple. A tuple is a promise that the parts are independent. Where that promise
  is false, the tuple *is* the defect, and every guard bolted onto it treats a symptom.
- **Invariant: `Residue` survives, and keeps its bomb, but its role is now a backstop.** It is
  reachable only through `take_residue()` — an explicit request to hold the obligation yourself.
  The other exit never constructs one, so two of the three former hazards became *unconstructible*
  rather than guarded. The last row of the table above is why the bomb stays: once deliberately
  taken out of the bundle, a `Residue` is a free-standing value again, and Rust has no linear
  types to stop you dropping it.
- **Note, accidental but load-bearing:** `const fn` and `Drop` are mutually exclusive (E0493).
  Keeping `take_residue`/`discard_deliberately` `const` therefore makes it a **compile error** for
  `Division` to ever grow a drop-bomb of its own. The property is enforced by the compiler rather
  than by anyone remembering it.
- **Invariant:** dropping a **nonzero, unabsorbed** `Residue` panics — in **every profile**, release included. In a ledger the residue must be *absorbed*: carried forward, posted to a rounding account, or handed to a party. Absorbing it means consuming the value (`.take_units()` and posting it, adding it back, or `.discard_deliberately()`). Letting it fall out of scope is money leaving the ledger, and it stops the program.
- **Rejected: counting the loss in release instead of panicking.** An earlier revision panicked in debug and incremented a process-global counter in release. Three things were wrong with it. It made the crate's central invariant **depend on the build profile**, so the release binary — the one handling real money — was the permissive one. The counter could not say *which* currency or *which* call site, so it was an alarm that could not be acted on. And reporting a loss after the fact is not a remedy: either the residue was absorbed, or there is a bug that must stop the program. The counter also carried its own defect, which is what exposed the design: it saturated the individual residue and then `fetch_add`ed, and **every** `Atomic*::fetch_add` wraps, so two individually in-range magnitudes could drive the counter to **exactly zero with money lost** (measured: `fetch_add(u64::MAX - 10)` then `fetch_add(100)` reads `89`). `AtomicU128` does not exist on stable, so a canonical-unit magnitude could not have been held losslessly anyway.
- **Invariant:** discarding requires `.discard_deliberately()` — explicit, greppable, auditable. Strictly this is an acknowledged **loss**, not an absorption: the money does not reach the ledger, the caller has accepted that. It is the one door this contract leaves open, deliberately and by name.
- **Failure:** Rust has no linear types, so a `Drop` impl cannot be *forbidden* — only made loud. That limit is real, but it now binds a **much smaller surface** than this contract used to claim: it applies only to a `Residue` the caller deliberately took out of a `Division`, not to the ordinary path. What the language does permit is making the *unwanted state unreachable*, which is what the bundle does.
- **Failure — the one hole, now narrowed:** the bomb must not fire while the thread is already unwinding, because a panic inside `Drop` during a panic **aborts the process**, destroying the original panic's diagnostics and preventing any rollback. So a residue dropped during an unwind still vanishes silently. Aborting is not failing *harder*, it is failing *worse*. The hole is unchanged in nature but much rarer in practice: it can only be reached by a caller who has already opted into holding the residue, since no other path constructs one.

**Restating the requirement honestly.** "Loses no money" is **literally unsatisfiable**: `10.00 / 3` is unrepresentable in any finite decimal at any width in any backing type. No canonical repr fixes that. The achievable property is **conservation** — rounding may occur, but residue is never *discarded* silently, and `allocate` sums back to the whole exactly. That is what this contract delivers.

### C6 — FX conversion, compile-time and runtime

```rust
pub struct Rate<Base, Quote> { units: i128, /* PhantomData<(Base, Quote)> */ }  // canonical SCALE

impl<C: StaticCurrency> Money<C> {
    fn convert<Quote: StaticCurrency>(self, rate: Rate<C, Quote>, mode: Rounding)
        -> Result<Money<Quote>, MoneyError>;                // NO residue — see below

    fn convert_via<Bridge: StaticCurrency, Quote: StaticCurrency>(
        self, first: Rate<C, Bridge>, second: Rate<Bridge, Quote>, mode: Rounding)
        -> Result<Money<Quote>, MoneyError>;
}

```

A runtime quote table needs no value-carrying rate type: keep the `(base, quote) -> units` map
private and expose a **generic accessor**, so the lookup is by code at runtime and the result is
typed at compile time.

```rust
impl QuoteTable {
    fn get<Base: StaticCurrency, Quote: StaticCurrency>(&self) -> Option<Rate<Base, Quote>>;
}
```

- **Invariant: a rate's two currencies are its BASE and its QUOTE**, which are the domain's words, not `from`/`to`, which were this crate's. The **base** is the currency a rate prices one unit of; the **quote** (or counter) is the currency that price is expressed in. `EUR/USD 1.2500` means one EUR buys 1.2500 USD. The same rule that governs C2's exponent and C7's alpha-3 governs this: where the domain has already named a thing, the crate uses that name rather than inventing a parallel one.
- **Invariant (2026-07-27, BREAKING, pre-1.0): a rate's units are STRICTLY POSITIVE.** `Rate::from_units` refuses zero and negatives as well as anything outside `DOMAIN_MAX`; `Rate::try_from_units` reports **which** rule was broken — `MoneyError::NonPositiveRate` or `MoneyError::DomainOverflow`, magnitude tested first so `i128::MIN` is reported as the magnitude bug it is while an in-domain `-2` is reported as the sign bug it is. The invariant has exactly one owner and every ingress reaches it: the text parser and both serde forms land on `try_from_units`, and `postgres-types` and sqlx land on the text parser. `kamu-money-core/tests/rate_ingress.rs` proves that funnel at each surface rather than arguing it, because an adapter that grew its own parse would enforce a weaker rule and nothing else in the tree would notice.

> **Correction (2026-07-27), and it reverses a decision this document recorded as settled.** C6 previously bounded a rate's MAGNITUDE and was silent on its sign, and on 2026-07-21 the operator chose that reading deliberately from an explicit two-option fork — full signed domain over a positive-only constructor — so that `Rate` would stay a plain fixed-point number exactly like `Money` rather than acquire an invariant the contract had not asked for. Sign was the quote feed's responsibility. The cost was documented **on the type** instead of enforced: a negative rate flips the sign of the money passing through it and a zero rate sends it to zero, both silently, both ordinary arithmetic on in-domain values with no overflow and no residue. A test (`a_negative_rate_flips_sign_and_a_zero_rate_sends_to_zero`) pinned that behaviour precisely so a later "defensive" guard could not be added without something going red.
>
> That decision named its own revisit condition — *"revisit if a feed is ever ingested without validation"* — and the 2026-07-27 audit is what showed the condition had been met **by this crate's own code**. `FromStr`, serde's `Deserialize`, `postgres-types`' `FromSql` and sqlx's `Decode` all build a `Rate` straight from untrusted bytes. Four of the feed adapters the responsibility had been delegated to ship in this repository, so "the feed validates its own signs" had quietly become "nobody does". The tripwire test fired as designed and has been inverted rather than deleted, with its history kept at the site: the record of why the old behaviour was chosen is what makes this a decision re-taken rather than a decision drifted past.
>
> The compile-time half was never in question and is unchanged. A phantom pair proves `Rate<USD, IDR>` is not `Rate<IDR, USD>`; it cannot prove a runtime number is positive, and runtime construction is what finishes that proof. If a signed scaling factor is ever wanted it is a different thing from a price and gets its own name — weakening `Rate` to obtain one would hand back the silent sign flip.

- **Note, because the distinction is load-bearing and currently invisible:** `MoneyError::ConversionOverflow { from, to }` keeps *direction* words deliberately. It names a **conversion**, which is an operation; `base`/`quote` name a **rate**, which is a price. They coincide here only because C6 omits `inverse()` and therefore stores every pair in both directions — hold a `USD/IDR` rate and convert `IDR → USD` and the base would be `USD` while the conversion's `from` would be `IDR`. Under this contract they cannot diverge, which is exactly why the wrong word was harmless, and why it would not have stayed harmless.
- **Correction (2026-07-22), and the reason it is written down:** the wire form for a pair was first proposed as `"USD/IDR …"` on the stated grounds that it is *"the standard's, not this crate's"* — the identical justification C7 uses for alpha-3 and ISO numeric. **That was false and was asserted without checking.** ISO 4217 standardises the codes; it does **not** standardise pair notation. A pair is conventionally written with a slash, but the slash "may be omitted, or replaced by either a dot or a dash." So the delimiter is this crate's choice and must be defended as a choice. What the check *did* establish is the vocabulary above, which the crate was getting wrong independently. Three corrections in this document now share one shape — E0119, E0207, and this — and all three were memory presented as fact.

> **Correction (2026-07-21), found by compiling it.** The block above previously read
> `impl<F: StaticCurrency, T: StaticCurrency> Money<F>`, which **does not compile**: `T` appears
> in neither the trait nor the self type, so it is an unconstrained type parameter (**E0207**).
> The target currency has to be bound on the *method*, and the same applies to `convert_via`'s
> `T`. This is the mirror image of the E0119 correction under C3 — there the document asserted a
> compile error that did not exist, here it asserted a signature that does not compile. Both were
> written from memory. A signature in prose is not a signature until a compiler has seen it.
>
> `Rate` also needs a `PhantomData<(F, T)>` that `Money` does not. `Money<C>` proves it uses `C`
> through a real field (`tag: C::Tag`, E11); `Rate`'s only field is the currency-agnostic
> `units`, so without the marker its parameters are unconstrained (**E0392**). That is the one
> structural difference between the two types, and it is why `Rate`'s `Copy`/`Clone`/`PartialEq`
> are hand-written for the same reason `Money`'s are.

- **Invariant:** `Money<USD>` converted by `Rate<USD, IDR>` yields `Money<IDR>`. The pair is **type-checked at compile time**; a mismatched pair does not compile.
- **Invariant: `Rate` shares `Money`'s scale.** It is not separately "scale 18" — there is one `SCALE` in the crate (C1), and a rate is a fixed-point number at it like everything else. `Rate` reuses `DOMAIN_MAX` unchanged.
- **Invariant: conversion is FALLIBLE and returns `Result`, and there is deliberately no `impl Mul`.** Domain overflow here is a *condition*, not a bug, which is precisely what separates it from `Add`. Measured against real pairs: `USD→IRR` at today's rate leaves the domain at a balance of **$2.38 trillion**, and `USD→ZWL` at the 2008 rate leaves it at a balance of **$100,000**. C4 rejects `Add for Money<Dyn>` because *"a `+` that can fail on currency mismatch is a lie"*; a `*` that fails on an ordinary hundred-thousand-dollar conversion is the same lie about a different failure. `Result` rather than `Option` so the error names which conversion overflowed.
- **Invariant: conversion returns NO `Residue`, and this is not an oversight.** A conversion divides by `POW10_SCALE`, so its remainder is *always* strictly less than one canonical unit — measured over 200,000 random pairs, worst loss `0.499999` money units, which is `0` as an integer count. A `Residue<T>` here would be `Residue::new(0, ())` in every case: it drops silently, the bomb never fires, `take_units()` always returns zero. `convert_via` divides by `POW10_SCALE²` and is the same.
- **Failure this prevents:** an always-empty `Residue` is worse than none. `#[must_use]` would force every caller of the crate's *most common* operation to absorb a value that is always nothing, training the reflex `let (m, _) = …` or a habitual `.discard_deliberately()` — and that reflex then carries to `div_int`, where the residue is real money. A safety device that cries wolf degrades itself everywhere it is used. The loss is real but **unrepresentable**: below `1e-18` of a currency unit, which the ledger cannot express, so there is nothing to absorb. That is what distinguishes it from `div_int`, whose small divisor leaves whole units behind.
- **Invariant: `convert_via` never materialises the intermediate.** `A → B → C` rounds **once**, at the end. This is not a precision optimisation — measured at realistic magnitudes, doing it as two sequential conversions is off by `4.885e-14` currency units, ten orders below anything a currency can express. It is a **ledger** requirement: two sequential conversions materialise an intermediate `Money<B>` balance that the holder never held, quantising it to a whole canonical unit on the way through. `convert_via` never creates that balance, so there is no moment at which a party appears to hold a currency they do not.
- **Invariant: `convert_via` needs one `checked_mul`, not staged rounding.** Verified analytically and over 300,000 full-domain trials: an in-domain result implies `m·r₁·r₂ ≤ 1e72 < I256::MAX`, so every valid result fits, and there were **zero** false rejects. An overflow can only occur when the result would have left the domain anyway, so rejecting it is correct rather than conservative.
- **Deliberate omission: no `inverse()`.** Real FX has bid and ask; inverting a rate is *financially* wrong, not merely imprecise. Each direction requires its own rate. This also dodges the round-trip precision problem (`inverse(inverse(r)) != r` at any fixed scale).
- **Deliberate omission: no `compose()`.** Killed by the same reasoning as `inverse()`, one step further out: composing two mid-rates fabricates a third that cannot be traded at, and a derived rate the holder does not hold is the *second number* shape §0.1 rejects. Measured as well as argued — a composed rate's error grows **linearly with the amount**, because rate error is relative and amplifies, while a sequential conversion's intermediate quantisation is absolute. Above ~1e6 units composing was strictly worse, and it degraded without bound. What callers actually want is `convert_via`.
- **Invariant: a runtime quote table needs no value-carrying rate type.** Keep the `(base, quote) -> units` map private and expose `get<Base, Quote>() -> Option<Rate<Base, Quote>>`. The lookup is by code at runtime; the result is typed at compile time; and a pair nobody stored is `None` *for a pair the type names*, so "no such quote" is distinguishable from "wrong pair". Demonstrated in `examples/fx.rs`.

> **Correction (2026-07-22): `AnyRate` and the `try_convert` pair are deleted, and the argument that justified them was mine and was wrong.** This contract previously held that `AnyRate` was *"load-bearing, not symmetry"*, citing a probe: typed rates alone would need one match arm per **ordered pair** — 132 for the 12 currencies defined, **32,220** at the full ISO register.
>
> **The measurement is real; the conclusion drawn from it is not.** It shows you cannot *enumerate* pairs as types. It does not show the runtime form must be a `Rate`. A private map behind a generic accessor satisfies the same requirement, refuses arithmetic entirely, and never lets a caller hold a rate whose pair the compiler does not know. The same 32,220 figure was then reused to argue for a runtime *money* type, where the relevant count is n = ~180 rather than n² — a second error, from the same number.

- **~~Decided: `Rate` keeps the full SIGNED domain, and the consequence is named rather than fixed.~~ SUPERSEDED 2026-07-27 — see the C6 correction.** Kept, not deleted, because the reasoning is what makes the reversal legible. Settled 2026-07-21 against the alternative of refusing `units <= 0` at construction. The contract bounds *magnitude* and says nothing about sign, so `Rate` stays a plain fixed-point number exactly like `Money` rather than acquiring an invariant this document never asked for. The cost is real and belongs on the record: a negative rate **flips the sign** of the money passing through it, and a zero rate sends it to **zero** — both silently, with no overflow and no residue, because both are ordinary arithmetic on in-domain values. Nothing in the type system reaches this; the sign of a quote is the quote feed's responsibility, and the crate documents it at `Rate` rather than pretending otherwise. **Revisit if a feed is ever ingested without validation.** — That last sentence is what closed it. The crate ships four such feeds (`FromStr`, serde, `postgres-types`, sqlx), each decoding untrusted bytes with no positivity check of its own, so the responsibility had been delegated to adapters that live in this repository and do not perform it. A rate is now strictly positive.
- **Note:** `Rate::from_units` is the only constructor phase 2 needs. Every real quote measured uses ≤13 decimal places against the 18 available, so decimal text is exact and needs no rounding mode; text ingestion is C7 wire work regardless.
- **Failure this prevents, measured 2026-07-21 while testing the above:** the narrowing from the `I256` quotient to `i128` must stay **checked**, and the domain check behind it does *not* cover it. Substituting a truncating `as_i128()` left the entire suite green, because the cases tried happened to wrap to values that were *still* out of domain, so `from_units` refused them anyway and the two gates were indistinguishable. They are not: a quotient of exactly `2^128` — reachable from an in-domain amount at an in-domain rate — truncates to **exactly zero**, and the conversion returns `Ok($0.00)` with the money simply gone. Pinned by `a_quotient_that_would_truncate_back_into_the_domain_is_still_refused`.
- **The small end, MEASURED 2026-07-22 — the estimate was right.** A rate is a count of `1e-18`, so its magnitude *is* its precision budget. Pinned by `tests/rate_small_end.rs`:

  | rate | units behind it | significant digits |
  |---|---:|---:|
  | `1.0` | `10^18` | 19 |
  | `1e-6` | `10^12` | 13 |
  | `1e-13` | `100000` | **6** |
  | `1e-17` | `10` | 2 |
  | `1e-18` | `1` | 1 |

  At `1e-13` the resolution is one part in `100000` — the seventh digit is not imprecise, it **does not exist**, and `from_units` cannot round to it. Because there is no `inverse()`, **every pair is stored in both directions**, so the tiny counter-direction of a hyperinflation quote is mandatory rather than hypothetical; a `USD→IDR` of `3e6` has a reverse of `3.33e-7` that still carries 12 digits, and a measured round trip drifts under `1e-6` of a major unit.

  The floor behaves, and is named rather than hidden: at a rate of `1e-18`, `1.0` converts to exactly one unit, and **anything below `1.0` converts to zero** — the rate has no digits left to carry it. That is ordinary truncation at the bottom of the domain, not a defect, but it is the point past which a quote should be rejected upstream rather than stored.

### C7 — Wire

**IMPLEMENTED.** Optional `serde` feature, default **off**; `kamu-money-core::wire`.

| Mode | `Money<USD>` | `Rate<USD, IDR>` |
|---|---|---|
| **structured** (default) | `{"currency":"USD","amount":"10.50"}` | `{"base":"USD","quote":"IDR","rate":"16000"}` |
| **transparent** | `"USD 10.50"` | `"USD/IDR/16000"` |
| **binary**, both modes | `(ISO numeric u16, i128 units)` | `(base u16, quote u16, i128 units)` |

```rust
#[derive(Serialize, Deserialize)]
struct Invoice {
    total: Money<IDR>,                                     // structured, the default
    #[serde(with = "kamu_money_core::wire::transparent")]
    tax: Money<IDR>,                                       // "IDR 176000.50"
}
```

Runnable: `cargo run -p kamu-money-core --example wire --features serde`.

- **Invariant: one trim rule for every human-readable form.** Render at 18dp, strip trailing zeros, **stop at the currency's ISO settlement exponent** (`None` for XAU/XDR/XXX → 0). Never round — padding is the only thing it adds, so §0.1 holds. `USD 10.50`, `JPY 10.5`, `JPY 10`, `KWD 10.500` are the same rule, not four cases. Demonstrated runnable in `examples/ledger.rs`.
  - The minimum is the **settlement** exponent, not a display one: C2 keeps display dp in `LocalePolicy` and off the wire, so the wire stays canonical and `LocalePolicy` stays off phase 3's critical path.
  - Render is canonical; **parse is liberal** (any exact decimal accepted). So the earlier claim that `String ↔ (i128, Iso4217)` is a **bijection** is weakened to `parse(render(v)) == v` — a retraction, since `"USD 10.5"` and `"USD 10.50"` both parse to one value.
- **Invariant:** binary carries the **ISO numeric code** ahead of the units — `(u16, i128)` for `Money`, `(u16, u16, i128)` for `Rate` — in both modes. This is a **reversal** (R2-F4's sibling, R2-F2): binary was a bare `i128`, "the currency costs zero bytes", until an external review showed that a bare `i128` is exactly what lets `Money<USD>` bytes decode as `Money<IDR>`. The type is chosen by the *reader*, independently of the writer, so serialization is the one boundary the compile-time currency does not cross. Two ISO-numeric bytes are the price of not silently redenominating money. The tag reuses the hand-written `Iso4217` numeric codec, so it inherits the ordinal-stability guarantee below.
- **Invariant:** deserializing `Money<USD>` from an IDR payload → `Err(WrongCurrency)`, in **both** modes — and this now genuinely includes binary, which before R2-F2 it could not, because a bare `i128` carried no identity to check. The redundancy catches an IDR value landing in a USD field at the API boundary, precisely where types cannot help. The same applies to `Rate`, on **both** ends of the pair; a swapped base or quote is refused, not reinterpreted.
- **Invariant: `Iso4217`'s codec is hand-written, never derived, and carries no `rename_all`.** Human-readable emits the alpha-3 code (`"IDR"`); binary emits the **ISO numeric** as `u16` (`360`). Both are assigned by the standard, so neither is this crate's choice to make.
- **Failure this prevents (measured, and the reason it is hand-written):** `#[derive(Serialize)]` encodes an enum variant by its **ordinal position**, not its discriminant. Inserting a currency mid-table shifts every later position, and stored `IDR` decoded as `GBP` — silently, with `#[repr(u16)]` and `IDR = 360` unchanged in both versions. `#[repr]` governs memory layout, not the wire. Human-readable formats emit the *name* and are unaffected, so a JSON test suite cannot catch this; it surfaces only when old binary data meets a newer build. The ISO numeric is immune because a standards body assigns it permanently.
- **Failure this prevents (measured):** `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` on `Iso4217` emits `"I_D_R"`. The trap is that it reads *more* correct than no attribute — "currency codes are screaming caps" is true, and the attribute asserting it corrupts every value. `UPPERCASE` happens to be correct, but hand-writing the codec removes the question entirely.
- **Invariant: no `Serialize`/`Deserialize` on `MoneyError`, `Rounding`, `Residue` or `Division`.** The first two are this crate's own vocabulary: a wire form for them is *our* house style shipped to every consumer, who would need a newtype to escape it. This crate promises a wire format for **money**, and for nothing else.
  - The last two are a stronger objection than style. **`Residue` is a drop-bomb**, so a `Deserialize` impl would let an attacker-controlled field materialise a panic-on-drop — a remote denial of service, not a preference. `Division` holds money that has not yet been accounted for and has no meaning outside the call that produced it.
- **Invariant: one parser, not two.** The wire's human-readable form goes through the same `text` module that backs `Display`/`FromStr`, which is **not** feature-gated. A second decimal reader would be a second set of rules, and the two would drift on exactly the inputs nobody tests.
- **Invariant: excess precision is REFUSED, never rounded**, and it has its own error variant so the refusal is greppable. This is the failure that disqualified `rust_decimal` for this crate (E2): its `from_str` silently rounded out-of-domain input and returned `Ok`.
- **Invariant:** two named modes, selected per field by `#[serde(with = ...)]` — `wire::transparent` (one scalar) and `wire::structured` (an object with named fields). **Structured is the default**, so the common case needs no attribute at all. Measured: a `with`-path typo is `E0433: cannot find module` at **compile** time, so per-field selection is already compile-checked and a proc-macro derive would buy nothing for a syn/quote dependency.
- **Rejected: a Cargo feature for rich-vs-transparent.** Cargo features are additive and unified across the dependency graph. Two crates wanting different wire formats would unify to one, silently, with no error. A wire format is the worst possible place for that. Per-field `#[serde(with = ...)]` gives the same compile-time selection with no global coupling.
- **Note:** features that only *add trait impls* (`serde`, `sqlx`, `postgres-types`) are safe — the hazard is behavior-changing features, not additive ones. That is why `serde` itself IS a Cargo feature while the *format* is not.
- **Hazard to document loudly for consumers:** `Iso4217` is a foreign type downstream, so a consumer who never finds the feature cannot write their own codec and would reach for `#[serde(remote = "Iso4217")]` — which reconstructs the variant-position corruption above. The feature needs to be discoverable in the README, not only in `Cargo.toml`.

**Why string at all:** JSON numbers are IEEE-754 doubles in JavaScript. `JSON.parse('{"x":10.500000000123}')` silently mangles it. The string is not a stylistic preference — it is the only exact decimal transport into JS. Therefore the string must be **canonical and exact**, not pretty. Pretty is `render(&LocalePolicy)`, a separate function that never touches the wire.

### C8 — PostgreSQL

```sql
CREATE EXTENSION kmoney;

CREATE TABLE ledger   (amount kmoney('IDR'));  -- single currency, pinned by typmod
CREATE TABLE payments (amount kmoney_mixed);   -- rows may differ in currency

SELECT sum(amount) FROM ledger;                -- the row aggregate: wide state, PARALLEL SAFE
SELECT kmoney_sum('IDR 1.00', 'IDR 2.50');     -- explicit values, the try_sum analogue
SELECT sum(amount) FROM payments;              -- ERROR at PLAN time: no sum(kmoney_mixed)
SELECT kmoney_from_mixed(amount, 'IDR') FROM payments;  -- checked conversion
```

`sum(kmoney)` **is** an aggregate, with a **wide** transition state — 32 bytes of `I256` plus the
two-byte ISO code, carried as `bytea`. R2-F4 removed an aggregate whose transition state was a
plain `kmoney`: that one re-checked the domain on every partial total, so a running sum that
transiently left the domain and returned failed or succeeded by plan order, and `PARALLEL = SAFE`
made the order a planner decision. **The narrow state was the defect, not the aggregate**, and
widening it is what makes a row total expressible again without reintroducing the plan dependence.

What the variadic form stood in for, it no longer has to. `kmoney_sum(VARIADIC array_agg(col))`
materialises every row into one PostgreSQL array before any arithmetic happens — memory linear in
the row count, on a type whose entire purpose is ledger columns, which meant a reconciliation over
a large table was the one shape this extension could not do. `kmoney_sum` remains as the
explicit-values operation, the exact analogue of Rust's `Money::try_sum`.

`bytea` for the state, rather than `internal` or a bespoke catalog type: `internal` would need a
serialize/deserialize pair before `PARALLEL = SAFE` could be declared honestly, and a bespoke type
would add a catalog entry whose text form is meaningless money. PostgreSQL copies a returned
`bytea` state into the aggregate context and frees the previous one, so memory is O(1) in the rows.

Cross-currency is refused at run time, the fastest way (a `u16` code compare against the first
operand), exactly as `+` refuses it. Both layers still share one kernel — `UnitSum` in
`kamu_money_core::arith`, which `sum_units` is now a fold over (C9). Rust keeps **no** `impl Sum`:
a fold through `+` is inherently narrow, and `Money::try_sum` is the wide form there.

**This block was wrong in three ways at once, and every one of them would have failed on the
first paste.** Kept as a correction rather than silently edited, because a contract whose
examples do not run is worse than a contract with no examples — a reader trusts it, and a
reviewer can approve behaviour the code does not provide.

| was | is | why |
|---|---|---|
| `CREATE EXTENSION exp_money` | `money_pg`, later `kmoney` | the extension is named by the control file's stem; `exp_money` never existed. The stem was renamed to `kmoney` on 2026-07-25 with the kamu rename -- recorded here rather than overwritten, because this row is a correction log |
| `kmoney(IDR)` | `kmoney('IDR')` | the typmod takes a **quoted** alpha-3; the tests and diagnostics have always used quotes |
| `CAST(amount AS numeric(36,18))` | *deleted* | there is deliberately **no** numeric cast — it was written and removed the same day, and a test now asserts its absence |
| `CAST(kmoney_mixed AS kmoney('IDR'))` | `kmoney_from_mixed(value, 'IDR')` | a cast to a typmod-bearing type cannot see the typmod, so the target currency has to be an argument |

- **Invariant:** both types are **native pgrx types storing the value directly** — fixed-size, no limb decode. `Decode` is a memcpy.
  - Payload is **18 bytes**, stored as `[u8; 16]` (units, little-endian) + `[u8; 2]` (ISO numeric code), and the column is **18 bytes on disk and in memory alike** — a fixed-length type with no varlena header, `typlen = 18, typbyval = f, typalign = c, typstorage = p`, which is `uuid`'s shape two bytes wider. **Measured — see E14.**
  - **The fields must not be `i128` and `u16`.** `i128` carries 16-byte alignment, which pads the struct to 32 (this is E8, measured in phase 1) *and*, worse, cannot be honoured: PostgreSQL's maximum alignment is `double` (8), pgrx declares no `ALIGNMENT` at all so the datum lands on a 4-byte boundary, and `PgVarlena::as_ref` casts that pointer straight to `&T`. A reference requiring 16-byte alignment built from 4-byte-aligned memory is undefined behaviour. Byte arrays make `align_of` 1, which any placement satisfies. Both facts are `const` assertions in `kamu-money-pg`.
  - **This contract previously read** *"PG imposes no Rust alignment rule, so the 32-byte in-memory figure of E8 does not apply on disk."* E8 was right, that sentence was wrong, and it was wrong in the most expensive possible way: PG imposing no alignment rule is precisely why the cast is unsound, because Rust's rule still applies to the reference pgrx manufactures. The measurement was in this document from phase 1 and a contract reasoned it away.
  - The currency must live in the *value* even for `kmoney`, because PG does not pass typmod to operators (below).
  - **Not a space optimisation.** `numeric(36,18)` is variable-width and beats a fixed 18 bytes for every amount short of the domain top — 7 bytes for `10.50` against 18 (E14). What this type buys is a value that cannot be stored without its currency, a width that does not move with the data, and the E13 refusal `numeric` cannot perform. Any size argument for C8 is unmeasured unless it cites E14's table.
- **Invariant: TWO types, and the second one has no arithmetic at all.** This is what makes a mixed column safe, and it is stronger than a runtime check:
  - `kmoney(IDR)` — operators and aggregates defined. Cross-currency between two differently-pinned columns still fails at **runtime**, on the value-carried code.
  - `kmoney_mixed` — **no `+`, no `-`, no `sum()` defined.** `SELECT sum(amount)` on such a column fails at **plan time** (`function sum(kmoney_mixed) does not exist`), before a single row is read. Not a runtime error on row 4,000,000 of a nightly batch.
  - This is the exact SQL analogue of the Rust design: `Add` exists only on `Money<C>`, so the unproven form cannot be added because *the impl is not there*. Same mechanism, both layers — which extends §0.1's *"calc in Rust == calc in PG"* to what is **forbidden**, not only to what is computed.
  - `CAST(kmoney_mixed AS kmoney(IDR))` is checked, and is the SQL twin of proving a value into a typed `Money<IDR>`.
  - **Naming:** not `kmoney_variadic`. `VARIADIC` is a PostgreSQL keyword meaning variable *arity*, and a reader would misparse it.
  - **Naming, second correction (2026-07-22):** these were `kmoney_t` and `kmoney_mixed_t` through every earlier revision of this contract. The `_t` suffix is a C convention and **no PostgreSQL type uses it** — not the built-ins (`numeric`, `jsonb`, `timestamptz`, `interval`), not the major extensions (`geometry`, `geography`, `vector`, `hstore`, `citext`, `ltree`). A column reading `amount kmoney_t` announces itself as a foreign object in a schema; `amount kmoney` reads like the rest of PostgreSQL. Changed while nothing is deployed, because a type name is the one part of an extension that cannot be revised later without a dump and restore.
- **Invariant:** our operators are the **only** operators for `kmoney`. There is no built-in `*` to accidentally reach. **The boundary rule disappears** rather than being policed.
- **Invariant:** for a single-currency column, currency is enforced at both levels:
  - `kmoney(IDR)` **typmod** → rejected on INSERT/coercion ≈ `Money<IDR>`
  - value-carried `Iso4217` → operator `ERROR` on mismatch, which is the only mechanism left once typmod is out of reach
  - **Failure:** PG does **not** pass typmod to operators. Typmod alone cannot make `kmoney(USD) + kmoney(IDR)` fail. Both mechanisms are required; neither is redundant.
- **Failure:** `Money<C>` is generic; PG types cannot be. So the currency is expressed in SQL by **typmod** (`kmoney(IDR)`) rather than by the type name, and the compile-time apparatus stays Rust-side.
- **Invariant: the schema declares heterogeneity, so Rust does not have to guess it.** A column's type says whether its rows share a currency. `kmoney(IDR)` decodes to `Money<IDR>` **via the canonical text form** (the C9 text adapters) — and "via text" means an **explicit server-side cast**, `SELECT amount::text`, because both adapters reject by OID before parsing, so a bare `SELECT amount` on a native column is a type error, not a negotiation. A Rust driver reads the native column as text, **not** through a native-OID binary codec (R2-F5); only `kmoney_mixed` needs a boundary form at all, and that form does not exist until there is a decoder to use it (C3). This is why the SQL surface is designed **before** the Rust types rather than after: C1 says the schema defines what money is.
- **Failure (one-way door):** pgrx requires loading a native extension. **RDS, Cloud SQL, Neon, and Supabase will not.** This design commits to self-hosted PostgreSQL, permanently.
- **Failure→Fixed (E15 measured the naive build; E16 supersedes the conclusion, 2026-07-24): YugabyteDB HOSTS this extension natively.** The *naive* build against the YB image fails exactly as E15 measured — its `elog.h` includes an undistributed `yb/yql/pggate/util/ybc_util.h`, and a foreign-built `.so` fails on the older glibc. But built FROM the YB image with a 3-symbol pgrx shim and `--cfg yb`, kamu-money-pg loads on YugabyteDB `2025.2.5.1-b1` (`PostgreSQL 15.12-YB`) and its ABI battery is **byte-exact** against stock PG15 — the F3 pinned `kmoney_hash` values match to the exact `i32`, and `COPY (FORMAT BINARY)` round-trips `kmoney_recv`. YugabyteDB is a first-class native target (`just yb-native` / `yb-ab`); the phase-4 text adapters remain the portability path for managed PostgreSQL that will not load a native extension.
- **Failure:** pgrx pins PG majors. A PG upgrade means rebuilding and redeploying the extension.
- **Note, revised 2026-07-22:** `NUMERIC(36,18)` survives as the **domain definition only** — the sentence that defines what a money value *is*. It is no longer a storage target, a cast target, or a wire form anywhere in this design, and the base-10000 limb codec it once justified is **deleted rather than deferred**. Operator ruling: *"NEVER use NUMERIC"*. The evidence agrees: E9 measured PG's `*`, `/` and `avg()` rounding silently at a value-dependent scale, and E13 measured `numeric(36,18)` rounding over-precise input on the way **in**, where no `CHECK` or `DOMAIN` can reach it. A type that cannot be written to safely is not a storage type.
- **Provenance.** E9 measured PG stating the bound for `numeric(36,12)`; **E13 has now measured `numeric(36,18)` directly**, and PG states it verbatim: *"A field with precision 36, scale 18 must round to an absolute value less than 10^18."* The derived rule was correct. No longer an open item.
- **Failure, measured, and the database CANNOT defend against it (E13):** PostgreSQL **silently rounds** over-precise input on the way into a `numeric(36,18)` column. `'0.0000000000000000004'` is stored as **zero**, with `INSERT 0 1` and no warning — the verbatim `rust_decimal` failure (E2) that this design rejected a dependency over, reproduced by the source of truth itself.
  - **No `CHECK` or `DOMAIN` can catch it**, because both run *after* the cast: the constraint is shown the already-rounded value (`DETAIL: Failing row contains (0.000000000000000000)`). A `CHECK (v <> 0)` rejects the round-to-zero case only by accident, and `5e-19 → 1e-18` passes every constraint while not being the value that was sent.
  - **Therefore the application boundary is the only place the loss is catchable**, which retroactively justifies `MoneyError::ExcessPrecision` as necessary rather than merely strict. Any write path bypassing `kamu-money-core` — an ad-hoc `INSERT`, a migration, an ETL job, another service on the same column — can silently alter an amount.
  - This is a **bounded exception to §0.1**: Rust and PG agree on arithmetic and disagree on ingestion of values that were never representable. Rust refuses; PG rounds.
- **Evidence:** E9.

**Requirement deviation, stated plainly — now two of them.** The original brief said *"In Database, the representation should be NUMERIC(36,12)."* Neither half of that survives verbatim. **(1) The scale changed**: money is `NUMERIC(36,18)`, so the domain is `|v| < 10^18` rather than `< 10^24`. That was accepted deliberately, to put money and rates on **one** scale — a schema holding both `numeric(36,12)` and `numeric(36,18)` asks a human to remember which column is which, and getting it wrong is a silent factor of `10^6` that no type reaches in a migration or an ad-hoc query. The cost is integer range, and it binds a single stored value rather than an aggregate: one row at the cap is ~$62.5 trillion. **(2) The type name changed**: the column is `kmoney(IDR)`. This was accepted knowingly: a native type is the only construction where SQL-side `*` resolves to our operator, and removing the boundary was the stated reason for choosing pgrx.

### C9 — Drivers

- `kamu-money-core` feature `sqlx` — `sqlx::Type` + `Encode` + `Decode`
- `kamu-money-core` feature `postgres` — `postgres-types::ToSql` + `FromSql`
- **Not separate crates.** `impl ToSql for Money<C>` from an external crate is **E0117** (foreign trait, foreign type), verified with a throwaway compile. Feature-gating in the crate that owns the type is what `chrono` and `uuid` do.
- **Invariant:** both are thin adapters over one codec. Neither reimplements semantics.
- **Invariant, settled 2026-07-22: the stored form is the CANONICAL TEXT FORM**, `kamu_money_core::text` — the same `"USD 10.50"` that `Display` prints, the serde wire carries, and `kmoney`'s in/out functions read. Not `numeric`, not a binary blob, not a bespoke encoding.
  - **It is exact.** Every in-domain value renders and re-parses to itself; the pair is a retraction, proven by proptest over the whole domain.
  - **It carries its currency**, so a stored amount cannot be separated from what it denominates — the same property `kmoney` buys at 18 bytes, available on a database that has never heard of this project.
  - **It is arithmetically inert.** A `text` column has no `*`, no `/`, no `avg()`. E9's boundary rule does not need policing because the operators do not exist. This is C8's *"the boundary rule disappears"* achieved by a different route.
  - **It cannot be silently corrupted on ingress.** E13 measured `numeric(36,18)` rounding `'0.0000000000000000004'` to zero on INSERT, uncatchable by `CHECK` or `DOMAIN` because constraints run after the cast. Text has no lossy cast to hide in.
  - **Cost, stated:** wider on disk than `numeric` for typical amounts, and unordered without a functional index. Both accepted. Correctness at the boundary was never going to be the cheap option, and E14 already established that this design does not compete on bytes.
- **Invariant: `Display` is now FROZEN.** It backs four consumers — `Display` itself, the serde wire (C7), `kmoney`'s in/out (C8), and this stored form. Any change to its output is a change to an on-disk format and a wire format simultaneously. Locale-dependent rendering (`LocalePolicy`, C2) must therefore be a **separate** entry point and may never route through `Display`.
- **Failure:** `sqlx`'s built-in `NUMERIC` decoder returns `Decimal` and is therefore **unusable** — it would reintroduce the E5 ceiling on the wire. Both adapters bypass it, and now have no reason to touch `NUMERIC` at all.
- **Consequence:** this is the portability route for managed PostgreSQL that will not load a native extension. It is **not** the only route to **YugabyteDB** — native `kmoney` runs there too (E16 supersedes E15's conclusion). Both adapters must still be exercised against YugabyteDB as well as PostgreSQL, and the two must agree with `kamu-money-pg` value-for-value (the phase 4 ↔ phase 5 differential).

### C10 — Integer conversion

- **Invariant: widening is implicit and infallible.** `From` / `i128::from`. Never `as`.
- **Invariant: narrowing is explicit and loud.** Three permitted shapes, in order of preference:
  1. return `Option`/`Result` — the caller must handle it (`Money::from_units`);
  2. `try_from(..).expect(<the proof>)` where a local invariant makes it total, **with the proof written at the site** (`allocate`, `div_int`);
  3. saturate **plus an observable signal** — only where the call site structurally cannot fail. As of C5's revision no site in the crate qualifies, and the one that did was deleted rather than fixed.
- **Invariant: re-encoding is not narrowing.** `i128 -> [u8; 16]` is a lossless bijection and must be **total and infallible** — no `Option`, no residue. (The `i128 -> i16` PG-limb half of this example died with the limb codec; the invariant did not, and `kmoney`'s little-endian payload is now its live instance.) `to_bytes` returns a fixed-size array, never `Vec<u8>`. Wrapping a lossless re-encoding in a fallible signature is its own lie, and it trains callers to `.unwrap()` the ones that matter.
- **Invariant: enforcement is `deny` in `lib.rs`, not `warn` plus a gate flag.** A flag lives in a shell history and a CI config: forgettable, editable, not shipped with the source. `clippy::all`, `pedantic` and `cargo` are denied wholesale. `restriction` and `nursery` are cherry-picked **by name** — not because their hits are bad (`use_self` and `missing_const_for_fn` are adopted) but because `restriction` is self-contradictory by design and `nursery` is under development, so denying either group lets a toolchain upgrade break every local build for reasons unrelated to this code. **Note (2026-07-24):** `kamu-money-pg` now satisfies this invariant too. It previously carried **no** crate-level `deny` at all and leaned on the Justfile passing `-D warnings` — exactly the gate flag this invariant rejects — so the casts in the FFI crate were governed by a recipe rather than by its own source. It now denies the same set in its own `lib.rs`. Adopting them immediately found a real latent defect: `b'c' as c_char` was a platform-dependent wrap, because `c_char` is `i8` on x86-64 but `u8` on some ARM targets; it is now a `try_from`.
- **Invariant: an `#[allow]` names its condition for removal.** Every one is scoped to an item or to a **single statement** — never to a function or a crate, because a function-wide allow blanket-permits the *next* unaudited cast in the very code that most needs auditing. The conversion- and memory-safety-relevant ones, as of 2026-07-24:
  - `kamu-money-core` `Iso4217::numeric` (emitted by the `kamu-money-iso` macro) allows `as_conversions`: the enum is `#[repr(u16)]` with explicit discriminants, so the cast reads a discriminant rather than narrowing a value, and `mem::discriminant` cannot produce the number.
  - `kamu-money-core/rounding.rs::div_round_i256` allows `arithmetic_side_effects`: measured, the lint fires on operator syntax for **any** type including `ethnum::I256`; the `assert!` already rules out div-by-zero, and an overflow would mean the caller violated the domain by dozens of orders first. Removable by rewriting the operators as `checked_*` — deliberately not done, because this primitive is transcribed exactly as specified.
  - `kamu-money-core/lib.rs` allows `cargo_common_metadata` because `Cargo.toml` has no `repository`, because the repo has no remote.
  - `kamu-money-pg` `kmoney_typmod_out` allows `as_conversions` / `cast_possible_truncation` / `cast_possible_wrap` on **one statement**: PostgreSQL stores "no typmod" as the sentinel `-1`, which arrives as `0xFFFF_FFFF_FFFF_FFFF` in the `Datum`'s `usize`. `as` reinterprets the low 32 bits and recovers `-1` exactly, while `i32::try_from` would *reject* it — the rare cast where the lint's suggested "safe" fix would be the bug. Removable never, short of PostgreSQL abandoning the sentinel.
  - `kamu-money-pg` `kmoney_typmod_in` allows `cast_ptr_alignment` on the `varlena -> ArrayType` cast: `pg_detoast_datum` returns palloc'd memory and palloc is MAXALIGN'd, so the pointer is **over**-aligned, never under. It is the same cast PostgreSQL's own `DatumGetArrayTypeP` performs. Removable if pgrx ever models the alignment.
  - `kamu-money-pg` `kmoney_sum` allows `needless_pass_by_value`: pgrx's `#[pg_extern]` ABI takes the owned `VariadicArray` to build the SQL wrapper. Removable if pgrx grows a by-reference form.
- **Failure: a width chosen against one end of a range quietly loses at the other.** `from_major(i64)` capped construction at ~9.2e18 against a domain of ~1e24 major units — **5.04 orders unreachable** — and its `Option` could never be `None`, which the doc presented as a virtue. The `Option` is the tell: a fallible signature that cannot fail is usually a range amputated to make it total.
- **Failure: a saturating guard on the individual value does not protect the accumulator.** Measured: `AtomicU64::fetch_add(u64::MAX - 10)` then `fetch_add(100)` reads `89`, and a counter can be driven to **exactly zero**. **Every** `Atomic*::fetch_add` wraps; none has a checked or saturating variant. Clamping into an accumulator needs a guard at *both* sites, and the justification comment naturally attaches to only one of them.
- **Failure: metadata asserting facts about the world has no compiler.** A fabricated `repository` URL builds, tests green, and ships; reviewers check config for format, not truth. Every such value — `repository`, `homepage`, `readme`, `authors`, `license` — is a claim to be verified with a command (`git remote -v`, `ls`) or escalated. Never invent one to satisfy a lint; omit the field, allow the lint, record why.
- **Evidence:** implemented across `d315e1d`..`f2f829f`; the phase-2a design record it was written from has been removed as archaeology.

---

## 5. Rejected Alternatives

| Rejected | Killed by |
|---|---|
| `Decimal` as the canonical representation | E3 — addition silently breaks the scale-12 invariant at IDR magnitudes |
| `Decimal` as a compute lens over `i128` | E5 — covers 0.000007923% of the domain; reintroduces the exact ceiling `i128` removed |
| Narrowing the column to `NUMERIC(29,12)` to fit `Decimal` | Caps at ~7.9e16 currency units (~8× Indonesia's M2 in IDR); still needs a scale re-assert after every add |
| Three backing types (`String` / `Decimal` / `i128`) | **The assistant's own first recommendation, and wrong.** Added six conversion edges — each a place money dies — and zero safety. The operator's one-canonical-repr instinct beat it. |
| Matching PG's arithmetic semantics from Rust | E9 — PG's division scale is value-dependent (`1/3` → 20, `10.00/3` → 16). Not a rounding mode; a fork of PG's internals. |
| `Rate::inverse()` | Financially wrong (bid ≠ ask), independent of precision |
| `Rate::compose()` | Fabricates a rate the holder does not hold — §0.1's *second number*, one step past `inverse()`. Measured: composed error grows **linearly with the amount**; strictly worse than sequential above ~1e6 units, unbounded thereafter. `convert_via` is what callers wanted |
| `impl Mul<Rate<Base,Quote>> for Money<Base>` | An operator that fails on ordinary input. Measured: `USD→ZWL` at the 2008 rate leaves the domain at a **$100,000** balance. Same objection C4 raises to `Add for Money<Dyn>` |
| `#[derive(Serialize)]` on `Iso4217` | serde encodes the variant **index**, not the discriminant. Measured: after inserting one currency mid-table, stored `IDR` decoded as `GBP`, silently, despite `#[repr(u16)]` and `IDR = 360` in both versions. The table is documented as growing, so the insert is the plan |
| `serde(rename_all)` on `Iso4217` | `SCREAMING_SNAKE_CASE` emits `"I_D_R"` — measured. It reads *more* correct in the source than it behaves. Any `rename_all` also imposes a convention on downstream consumers of a published crate |
| Cargo feature for wire format | Feature unification silently picks one across the dep graph |
| `ruint` / `primitive-types` / `crypto-bigint` for the 256-bit intermediate | Unsigned-only; cannot hold a negative balance |
| ISO 4217 `Custom(Alpha3)` escape hatch | Reopens the closed set; `exponent()` becomes unknowable |

---
