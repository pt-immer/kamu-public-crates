# Changelog — `kamu-snap-response-axum`

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
