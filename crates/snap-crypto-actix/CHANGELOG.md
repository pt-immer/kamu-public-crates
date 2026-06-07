# Changelog — `kamu-snap-crypto-actix`

## 2.0.0 — 2026-05-28

### Added

- Initial release: `verify_request(method, path, headers, body, client_secret)`
  inbound-verify helper bridging actix-web's `Method` / `HeaderMap` to
  `kamu_snap_crypto::snap_bi::verify_service`.
- `#![forbid(unsafe_code)]`.

First version published from the `pt-immer/kamu-public-crates` workspace
(relicensed `MIT OR Apache-2.0`; previously MIT-only in `pt-immer/lib-snap`).
