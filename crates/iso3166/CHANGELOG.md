# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
