# kamu-public-crates

[![CI](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml/badge.svg)](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml)
[![Release](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml/badge.svg)](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml)

A Cargo workspace of small, focused Rust crates — libraries and CLI apps — published by PT IMMER.

## Crates

| Crate                                | Description                                                                 | crates.io |
| ------------------------------------ | --------------------------------------------------------------------------- | --------- |
| [`kamu-iso3166`](crates/iso3166)     | Zero-allocation, `no_std` ISO 3166-1 / 3166-2 country & subdivision primitives | [![v](https://img.shields.io/crates/v/kamu-iso3166.svg)](https://crates.io/crates/kamu-iso3166) |
| [`kamu-logging`](crates/logging)     | Structured logging over the `tracing` ecosystem: systemd/journald, Cloudflare-Worker `wasm32`, `actix-web` spans, OpenTelemetry/OTLP | [![v](https://img.shields.io/crates/v/kamu-logging.svg)](https://crates.io/crates/kamu-logging) |
| [`kamu-snap-crypto`](crates/snap-crypto) | Bank Indonesia SNAP BI cryptography: HMAC/RSA primitives, signing recipes, webhook verifier (framework-free leaf) | [![v](https://img.shields.io/crates/v/kamu-snap-crypto.svg)](https://crates.io/crates/kamu-snap-crypto) |
| [`kamu-snap-response`](crates/snap-response) | SNAP BI response envelope + 61-variant error taxonomy (framework-free leaf) | [![v](https://img.shields.io/crates/v/kamu-snap-response.svg)](https://crates.io/crates/kamu-snap-response) |
| [`kamu-snap-crypto-actix`](crates/snap-crypto-actix) | actix-web inbound-verify helper for `kamu-snap-crypto` | [![v](https://img.shields.io/crates/v/kamu-snap-crypto-actix.svg)](https://crates.io/crates/kamu-snap-crypto-actix) |
| [`kamu-snap-crypto-axum`](crates/snap-crypto-axum) | axum/`http` inbound-verify helper for `kamu-snap-crypto` | [![v](https://img.shields.io/crates/v/kamu-snap-crypto-axum.svg)](https://crates.io/crates/kamu-snap-crypto-axum) |
| [`kamu-snap-response-actix`](crates/snap-response-actix) | actix-web `Responder` adapter for `kamu-snap-response` | [![v](https://img.shields.io/crates/v/kamu-snap-response-actix.svg)](https://crates.io/crates/kamu-snap-response-actix) |
| [`kamu-snap-response-axum`](crates/snap-response-axum) | axum `IntoResponse` adapter for `kamu-snap-response` | [![v](https://img.shields.io/crates/v/kamu-snap-response-axum.svg)](https://crates.io/crates/kamu-snap-response-axum) |

Each crate versions and releases independently — see its own `CHANGELOG.md`.

## Layout

```text
kamu-public-crates/
├── Cargo.toml            # workspace: shared package metadata, deps, lints
├── crates/
│   ├── iso3166/          # kamu-iso3166 (vendors ISO data as a git submodule)
│   ├── logging/          # kamu-logging
│   ├── snap-crypto/      # kamu-snap-crypto (SNAP BI crypto, leaf)
│   ├── snap-response/    # kamu-snap-response (SNAP BI envelope/errors, leaf)
│   ├── snap-crypto-actix/    # actix-web verify adapter
│   ├── snap-crypto-axum/     # axum/http verify adapter
│   ├── snap-response-actix/  # actix-web Responder adapter
│   └── snap-response-axum/   # axum IntoResponse adapter
└── .github/workflows/    # on-pr-synced.yml, on-release-published.yml
```

## Getting started

`kamu-iso3166` vendors its ISO 3166 dataset as a git submodule, so clone
recursively (or initialize the submodule after cloning):

```sh
git clone --recurse-submodules https://github.com/pt-immer/kamu-public-crates.git
# or, after a plain clone:
git submodule update --init --recursive
```

Common tasks are wrapped in a [`Justfile`](Justfile):

```sh
just            # list recipes
just gate       # complete CI-equivalent barrier — run before pushing
just check-all  # fast inner loop: fmt + clippy + test
just ci         # gate + publish dry-run (the full pipeline)
just test-all   # workspace tests + every crate's feature permutations
just cov-all    # coverage gates for every gated crate
just lint-all   # rustfmt + clippy + Markdown + TOML + spelling
```

Tests run under [cargo-nextest](https://nexte.st/) (installed by `just setup`,
configured in `.config/nextest.toml`). It runs each test in its own process and
does not run doctests, so doctests are always a separate pass.

Without `just`:

```sh
cargo nextest run --workspace
cargo test --workspace --doc
cargo nextest run -p kamu-iso3166 --all-features
cargo clippy --workspace --all-targets -- -D warnings -D clippy::all
```

> `--all-features` is **not** valid across the whole workspace: `kamu-logging`'s
> `systemd` and `wasm32` features are mutually exclusive. Use per-crate feature
> selection (as the recipes and CI do).

## Releasing

Releases are per-crate. Tag a GitHub Release `<crate>-vX.Y.Z` (e.g.
`kamu-iso3166-v0.2.0`); the `on-release-published` workflow verifies the
manifest version matches the tag and publishes that single crate to crates.io.
See [CONTRIBUTING.md](CONTRIBUTING.md).

## MSRV

Rust **1.94** (workspace-wide), exercised in CI alongside `stable`.

## License

Crate source code is dual-licensed under either of [Apache License, Version
2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

`kamu-iso3166` additionally embeds ISO 3166 data under CC BY-SA 4.0; see its
[`NOTICE`](crates/iso3166/NOTICE) and [`VENDORED.md`](crates/iso3166/VENDORED.md).
