# Changelog — `kamu-snap-crypto-axum`

## 3.0.0 — 2026-07-28

### Breaking

- `verify_request` returns the structured
  `kamu_snap_crypto::snap_bi::ServiceVerificationError`.
- Authorization now requires exactly one Bearer credential. Raw, Basic, empty,
  and multi-token values no longer reach signature verification.

### Changed

- Header extraction is the adapter's only policy; authorization,
  canonicalization, signature decoding, and HMAC verification delegate to
  `kamu-snap-crypto`.
- Added behavior tests for authorization parity, missing headers, and BRI's
  query-excluded canonical path.
- Added a bounded-buffering regression proving an over-limit body never reaches
  signature verification as empty bytes.

### Fixed

- The README now uses a finite body limit and propagates body-read failure
  instead of verifying an empty fallback.

## 2.2.0 — 2026-07-27

Toolchain maintenance only. No code or public API changes.

### Changed

- Minimum supported Rust version raised to 1.94.

## 2.1.0 — 2026-06-11

Toolchain/metadata only. No code or public API changes.

### Changed

- MSRV raised from 1.85 to 1.88 (workspace-wide, driven by the workspace
  `time` >= 0.3.47 floor for RUSTSEC-2026-0009). This crate has no `time`
  dependency itself.

## 2.0.1 — 2026-06-08

Docs only. No code or public API changes.

### Changed

- Standardized the README badge block (Crates.io / docs.rs / CI / License / MSRV)
  and added a workspace link.

## 2.0.0 — 2026-05-28

### Added

- Initial release: `verify_request(parts, body, client_secret)` inbound-verify
  helper operating on `http::request::Parts`, delegating to
  `kamu_snap_crypto::snap_bi::verify_service`. Depends only on `http` (no axum
  /tower dep).
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
