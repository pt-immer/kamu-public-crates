# Changelog — `kamu-snap-response-axum`

## 4.0.1 — 2026-08-19

### Changed

- The internal-error body comes from `kamu_snap_response::internal_error_body`
  rather than a private copy. No wire change for service code `00`.

## 4.0.0 — 2026-07-28

### Breaking

- Updated to `kamu-snap-response` 3 and its valid-by-construction response API.

### Fixed

- Serialize the JSON body before applying the intended HTTP status. A
  serialization failure now returns HTTP 500 instead of allowing Axum's tuple
  status override to restore a success status.
- Require a fully valid `HHHSSCC` code before selecting the framework status.

### Added

- Boundary tests for success, failure, malformed service digits, reserved
  payload keys, serialization failure, JSON body, and content type.

## 3.1.0 — 2026-07-27

Toolchain maintenance only. No code or public API changes.

### Changed

- Minimum supported Rust version raised to 1.94.

## 3.0.0 — 2026-06-11

### Changed

- **Breaking:** axum bumped from 0.7 to 0.8. axum is a public dependency
  (`IntoResponse` / `Json` appear in this crate's API), so consumers must be on
  axum 0.8 to use this release. No source changes were required — the
  `AxumResponder<T>` / `SnapResponderExt` API is identical.

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

- Initial release: `AxumResponder<T>` newtype + `SnapResponderExt::into_axum`,
  giving `axum::response::IntoResponse` (axum 0.7+) for
  `kamu_snap_response::SnapResponse<T>`.
- Defensive fallback to `500 INTERNAL_SERVER_ERROR` on malformed `responseCode`.
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
