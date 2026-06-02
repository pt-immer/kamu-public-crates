# kamu-libs

[![CI](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml/badge.svg)](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml)

A Cargo workspace of small, focused Rust libraries published by PT IMMER.

## Crates

| Crate                                | Description                                                                 | crates.io |
| ------------------------------------ | --------------------------------------------------------------------------- | --------- |
| [`kamu-iso3166`](crates/iso3166)     | Zero-allocation, `no_std` ISO 3166-1 / 3166-2 country & subdivision primitives | [![v](https://img.shields.io/crates/v/kamu-iso3166.svg)](https://crates.io/crates/kamu-iso3166) |
| [`kamu-logging`](crates/logging)     | Structured logging helper over the `tracing` ecosystem (systemd / wasm / actix) | [![v](https://img.shields.io/crates/v/kamu-logging.svg)](https://crates.io/crates/kamu-logging) |

Each crate versions and releases independently — see its own `CHANGELOG.md`.

## Layout

```text
kamu-libs/
├── Cargo.toml            # workspace: shared package metadata, deps, lints
├── crates/
│   ├── iso3166/          # kamu-iso3166 (vendors ISO data as a git submodule)
│   └── logging/          # kamu-logging
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
just check-all  # lint-all + test-all + cov-all + doc + cross builds + deny
just ci         # check-all + publish dry-run (the full pipeline)
just test-all   # workspace tests + kamu-iso3166 feature permutations
just cov-all    # coverage gates for both crates
just lint-all   # rustfmt + clippy + Markdown + TOML + spelling
```

Without `just`:

```sh
cargo test --workspace
cargo test -p kamu-iso3166 --all-features
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

Rust **1.85** (workspace-wide), exercised in CI alongside `stable`.

## License

Crate source code is dual-licensed under either of [Apache License, Version
2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

`kamu-iso3166` additionally embeds ISO 3166 data under CC BY-SA 4.0; see its
[`NOTICE`](crates/iso3166/NOTICE) and [`VENDORED.md`](crates/iso3166/VENDORED.md).
