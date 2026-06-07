# Changelog — `kamu-snap-crypto-axum`

## 2.0.0 — 2026-05-28

### Added

- Initial release: `verify_request(parts, body, client_secret)` inbound-verify
  helper operating on `http::request::Parts`, delegating to
  `kamu_snap_crypto::snap_bi::verify_service`. Depends only on `http` (no axum
  /tower dep).
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
