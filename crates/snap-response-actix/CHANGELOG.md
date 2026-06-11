# Changelog — `kamu-snap-response-actix`

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
