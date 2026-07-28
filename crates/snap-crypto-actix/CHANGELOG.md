# Changelog — `kamu-snap-crypto-actix`

## 3.0.0 — 2026-07-28

### Breaking

- `verify_request` returns the structured
  `kamu_snap_crypto::snap_bi::ServiceVerificationError`.
- Authorization now requires exactly one Bearer credential. Raw, Basic, empty,
  and multi-token values no longer reach signature verification.

### Changed

- Header extraction and Actix method conversion are the adapter's only policy;
  authorization, canonicalization, signature decoding, and HMAC verification
  delegate to `kamu-snap-crypto`.
- Added behavior tests for valid mixed-case Bearer schemes, invalid
  authorization shapes, and missing headers.

## 2.2.0 — 2026-07-27

Toolchain maintenance only. No code or public API changes.

### Changed

- Minimum supported Rust version raised to 1.94.

## 2.1.0 — 2026-06-11

Dependency/toolchain release. No code or public API changes.

### Changed

- MSRV raised from 1.85 to 1.88.
- Transitive `time` (via `actix-web` -> `cookie`) now resolves to >= 0.3.47,
  the RUSTSEC-2026-0009 / CVE-2026-25727 fix (RFC 2822 stack-exhaustion DoS).

## 2.0.1 — 2026-06-08

Docs only. No code or public API changes.

### Changed

- Standardized the README badge block (Crates.io / docs.rs / CI / License / MSRV)
  and added a workspace link.

## 2.0.0 — 2026-05-28

### Added

- Initial release: `verify_request(method, path, headers, body, client_secret)`
  inbound-verify helper bridging actix-web's `Method` / `HeaderMap` to
  `kamu_snap_crypto::snap_bi::verify_service`.
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
