# Changelog

This workspace versions and releases each crate independently. See the
per-crate changelogs for details:

- [`kamu-iso3166`](crates/iso3166/CHANGELOG.md)
- [`kamu-logging`](crates/logging/CHANGELOG.md)
- [`kamu-money-core`](crates/money-core/CHANGELOG.md)
- [`kamu-snap-crypto`](crates/snap-crypto/CHANGELOG.md)
- [`kamu-snap-response`](crates/snap-response/CHANGELOG.md)
- [`kamu-snap-crypto-actix`](crates/snap-crypto-actix/CHANGELOG.md)
- [`kamu-snap-crypto-axum`](crates/snap-crypto-axum/CHANGELOG.md)
- [`kamu-snap-response-actix`](crates/snap-response-actix/CHANGELOG.md)
- [`kamu-snap-response-axum`](crates/snap-response-axum/CHANGELOG.md)

## Unreleased

- Move the PostgreSQL/YugabyteDB extension to the standalone
  [kamu-money-pg repository](https://github.com/pt-immer/kamu-money-pg).
- Remove extension builds, tool pins and release checks from this workspace.
- Update locked rustls for RUSTSEC-2026-0285 and replace the yanked chacha20 release.
