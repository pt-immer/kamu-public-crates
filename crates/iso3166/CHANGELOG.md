# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0]

Toolchain maintenance only. No code or public API changes.

### Changed

- Minimum supported Rust version raised to 1.94.

## [0.3.1]

Dependency maintenance only. No code or public API changes (the `phf` lookup
tables are `pub(crate)`, so the bump is an internal implementation detail).

### Changed

- Bumped `phf` and `phf_codegen` from 0.13 to 0.14 (build-time codegen +
  runtime lookup). Codegen invariant counts (249 countries / 5046 subdivisions)
  unchanged.

## [0.3.0]

Toolchain/metadata only. No code or public API changes.

### Changed

- MSRV raised from 1.85 to 1.88 (workspace-wide, driven by the workspace
  `time` >= 0.3.47 floor for RUSTSEC-2026-0009). This crate has no `time`
  dependency itself.

## [0.2.2]

Docs/metadata only. No code or public API changes.

### Changed

- README badges converted to the workspace-standard reference style (Crates.io /
  docs.rs / CI / License / MSRV), dropping the separate Release badge.

## [0.2.1]

Docs/metadata only. No code or public API changes.

### Fixed

- README CI badge pointed at the release workflow (mislabeled); split into a CI
  badge (`on-pr-synced.yml`) and a Release badge (`on-release-published.yml`).

## [0.2.0]

Moved into the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

### Changed

- **Breaking:** module `one` renamed to `country` and `two` to `subdivision`.
  The old paths remain as `#[deprecated]` aliases and will be removed later.
- Dual-licensed under `MIT OR Apache-2.0` (previously Apache-2.0 only). The
  vendored ISO 3166 data remains under CC BY-SA 4.0.

### Added

- `Alpha2::iter()`, `Alpha3::iter()`, and `Subdivision::iter()`.
- `Numeric::new(u16) -> Option<Self>`, a `const`, `Option`-returning constructor.
- Extreme test coverage: exhaustive sweeps over all countries and subdivisions,
  property-based parser fuzzing, and pinned codegen-cardinality guards.

## [0.1.0]

Initial release.

### Added

- ISO 3166-1 primitives: `Alpha2`, `Alpha3`, `Numeric`.
- Total, infallible conversions between all three representations.
- Case-insensitive, zero-allocation parsers for every type.
- ISO 3166-2 `Subdivision` type with per-country accessors and full-code
  lookup.
- `Category` enum (`#[non_exhaustive]`) with an `Other(&'static str)` fallback
  for upstream additions.
- Optional `serde` integration behind the `serde` feature flag.
- `no_std` support; `std` feature is default-on for convenience.
- Data vendored from `ipregistry/iso3166` at SHA
  `1224d32fecbec52b21dc5b18e327fa9c09cb1c92`; see `NOTICE` and `VENDORED.md`.
