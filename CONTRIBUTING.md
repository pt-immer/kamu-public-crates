# Contributing to kamu-public-crates

Thanks for contributing! This is a Cargo workspace; each crate under `crates/`
is published independently to crates.io.

## Setup

```sh
git clone --recurse-submodules https://github.com/pt-immer/kamu-public-crates.git
# or after a plain clone:
git submodule update --init --recursive
```

`kamu-iso3166` reads its vendored ISO 3166 CSVs (a git submodule) at build time,
so the submodule must be initialized before building it.

## Before opening a PR

Run the same checks CI does:

```sh
just ci          # fmt-check + clippy + test + doc + deny + nostd
just cov         # kamu-iso3166 coverage gate (>= 95% lines)
```

or the raw commands — see the [`Justfile`](Justfile). The PR pipeline
(`on-pr-synced.yml`) runs rustfmt, clippy (`-D warnings -D clippy::all`
workspace-wide, plus `-D clippy::pedantic` for `kamu-iso3166`), tests on
`stable` and the `1.85` MSRV, a `no_std` cross-compile, docs, `cargo-deny`,
per-crate `publish --dry-run`, and coverage.

> Do not use `--all-features` across the whole workspace: `kamu-logging`'s
> `systemd` and `wasm32` features are mutually exclusive. Select features
> per crate.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`,
`fix:`, `chore:`, `docs:`, `refactor:`, `test:`, optionally scoped, e.g.
`feat(iso3166): add Alpha2::iter()`.

## Releasing a crate

Releases are **per crate**, with independent versions.

1. Bump the crate's `version` in its `Cargo.toml` and update its `CHANGELOG.md`.
2. Merge to `main`.
3. Create a GitHub Release with tag `<crate>-vX.Y.Z`
   (e.g. `kamu-iso3166-v0.2.0` or `kamu-logging-v0.1.5`).
4. `on-release-published.yml` verifies the manifest version matches the tag and
   runs `cargo publish -p <crate>`.

## Updating the vendored ISO 3166 data

See [`crates/iso3166/VENDORED.md`](crates/iso3166/VENDORED.md). Remember to
update the pinned cardinalities in `crates/iso3166/tests/codegen_invariants.rs`
if the dataset changes.

## License

By contributing you agree that your contributions are dual-licensed under
`MIT OR Apache-2.0`.
