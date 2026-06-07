# Changelog — `kamu-snap-response-actix`

## 2.0.0 — 2026-05-28

### Added

- Initial release: `ActixResponder<T>` newtype + `SnapResponderExt::into_actix`,
  giving `actix_web::Responder` for `kamu_snap_response::SnapResponse<T>`.
- Defensive `.http().unwrap_or(500)` — no `.unwrap()` on the response path.
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
