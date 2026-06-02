# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Moved into the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.
- Dual-licensed under `MIT OR Apache-2.0` (previously MIT only).
- Adopted the shared workspace lints (`-D warnings`, `-D clippy::all`).

### Added

- Unit tests for the `Error` type and the `init()` idempotency contract.

## [0.1.4]

### Added

- Include structured fields in the journald `MESSAGE` output.

## [0.1.3]

### Added

- Cloudflare Worker / `wasm32` compatibility.

## [0.1.2]

Initial published baseline of the systemd + actix-web logging helper.
