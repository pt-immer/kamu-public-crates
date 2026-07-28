# Changelog — `kamu-money-core`

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-07-28

Initial release.

### Added

- `Money<C>` — an exact monetary quantity held as `i128` canonical units at a
  fixed scale of 18, bounded to the magnitude of PostgreSQL's `NUMERIC(36,18)`.
  The currency lives in the type, so a cross-currency operation is a compile
  error and the value is exactly 16 bytes.
- `StaticCurrency`, sealed, with one zero-sized marker type per ISO 4217 code.
  Downstream crates cannot implement it, so a counterfeit currency cannot
  impersonate a genuine one.
- `Iso4217` and the full 178-code register, generated at build time from the
  vendored SIX Group list. The publication date, row counts, numeric uniqueness
  and internal consistency are checked while the register is read, so a replaced
  or edited file fails the build rather than a test.
- Exact `Add` and `Sub` that cannot round, panicking on domain overflow, with
  checked alternatives. `Money::try_sum` accumulates through a wider type and
  returns `Result`; `Sum` is deliberately not implemented, because fold order
  can otherwise create a transient overflow that the inputs and the output do
  not explain.
- `Division` and `Residue` — lossy division yields a `Division` that will not
  release its quotient until the caller takes the residue or discards it
  deliberately. A bare residue is a `#[must_use]` accounting obligation; its
  `Drop` never panics.
- `Rate<Base, Quote>`, strictly positive, rejecting zero and negative values at
  every ingress. There is deliberately no `inverse()` and no `compose()`.
- Result-based `try_from_units` and `try_from_major` constructors. Narrow
  `AmountError`, `ParseMoneyError`, `AllocationError`, `RateError`,
  `LocaleError`, and `WireError` contracts replace one catch-all error.
- Fallible `allocate` for conserving integer-weight distributions. `split`
  yields an allocation-free iterator; `split_collect` makes eager allocation
  explicit and reports reservation failure.
- Validated `LocalePolicy` configuration and `FractionDigits`. Empty decimal
  separators, equal non-empty separators, zero grouping widths, and widths
  above the fixed scale are rejected. Display pads; it never rounds.
- `stable_hash` — a hash of the canonical payload whose value is fixed by this
  crate rather than by a toolchain, for anything that persists it.
- Optional `serde` (structured, plus a transparent-string adapter), `postgres`
  and `sqlx` adapters. The database adapters live here rather than in sibling
  crates because `impl ToSql for Money<C>` from an external crate is `E0117`.

### Notes

- The ISO 4217 register in `vendor/list-one.xml` is third-party data published
  by SIX Group AG and is not covered by this crate's licence. See `NOTICE` and
  `VENDORED.md`.
- This crate was developed in a separate repository and re-homed into
  `kamu-public-crates` before its first release. The build-time register
  generator was a separately published procedural-macro crate there; it emitted
  code naming crate-private items and so could only ever compile inside this
  crate, and is now a build script instead.
