# Changelog — `kamu-snap-response-actix`

## 3.0.0 — 2026-07-28

### Breaking

- Updated to `kamu-snap-response` 3 and its valid-by-construction response API.

### Fixed

- Serialize the JSON body before applying the intended HTTP status. A
  serialization failure now returns HTTP 500 instead of retaining a success
  status.
- Require a fully valid `HHHSSCC` code before selecting the framework status.

### Added

- Boundary tests for success, failure, malformed service digits, reserved
  payload keys, serialization failure, JSON body, and content type.

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

- Initial release: `ActixResponder<T>` newtype + `SnapResponderExt::into_actix`,
  giving `actix_web::Responder` for `kamu_snap_response::SnapResponse<T>`.
- Defensive `.http().unwrap_or(500)` — no `.unwrap()` on the response path.
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
