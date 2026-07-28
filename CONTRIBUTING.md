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
just gate        # published-crate local barrier; CI adds Docker/package checks
just ci          # Docker-free gate + metadata-derived publish dry-runs
```

`just check-all` is the fast inner-loop check (fmt + clippy + test only) — handy
while iterating, but run `just gate` before you push.

or the raw commands — see the [`Justfile`](Justfile). The PR pipeline
(`on-pr-synced.yml`) runs rustfmt, clippy (`-D warnings -D clippy::all`
workspace-wide, plus `-D clippy::pedantic` for `kamu-iso3166`), tests on
`stable` and the `1.94` MSRV, a `no_std` cross-compile, docs, `cargo-deny`,
per-crate `publish --dry-run`, a `wasm32` build, Markdown/TOML/spelling lint,
and coverage (`kamu-iso3166` ≥ 98% lines, `kamu-logging` ≥ 88%,
`kamu-money-core` ≥ 80%, `kamu-snap-crypto` ≥ 70%,
`kamu-snap-response` ≥ 85%).

> Do not use `--all-features` across the whole workspace: `kamu-logging`'s
> `systemd` and `wasm32` features are mutually exclusive. Select features
> per crate.

Tests run under [cargo-nextest](https://nexte.st/) — locally, in `just gate`, in
coverage, and in CI — configured in `.config/nextest.toml`. `just setup`
installs it. nextest runs each test in its own process, and it does **not** run
doctests, so every recipe pairs a nextest run with an explicit
`cargo test --doc`; keep that pair when adding one.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`,
`fix:`, `chore:`, `docs:`, `refactor:`, `test:`, optionally scoped, e.g.
`feat(iso3166): add Alpha2::iter()`.

Work is tracked in JIRA under the `kec-` prefix. Name branches
`<type>/kec-<n>-<slug>`, and end every commit message with the lowercase ticket
on its own line — above any `Co-Authored-By:` trailer, which git only reads
from the final paragraph:

```text
chore(deps): refresh workspace dependencies

Bump every workspace requirement to the latest version the MSRV-1.94
resolver allows.

kec-1
```

## Releasing a crate

Releases are **per crate**, with independent versions.

1. Bump the crate's `version` in its `Cargo.toml` and update its `CHANGELOG.md`.
2. Merge to `main`.
3. Create a GitHub Release with tag `<crate>-vX.Y.Z`
   (e.g. `kamu-iso3166-v0.2.0` or `kamu-logging-v0.1.5`).
4. `on-release-published.yml` verifies the manifest version matches the tag and
   runs `cargo publish -p <crate>`.

The `kamu-snap-*` crates inter-depend, so release them in dependency order —
`kamu-snap-crypto` → `kamu-snap-response` → the four
`kamu-snap-{crypto,response}-{actix,axum}` adapters — waiting for the crates.io
index between tiers (cargo cannot package a crate whose deps, even optional ones,
are not yet published; the release workflow fails fast if you skip ahead). The
first publish of a brand-new crate also needs
`cargo owner --add github:pt-immer:rust-devs <crate>`.

## Updating the vendored ISO 3166 data

See [`crates/iso3166/VENDORED.md`](crates/iso3166/VENDORED.md). Remember to
update the pinned cardinalities in `crates/iso3166/tests/codegen_invariants.rs`
if the dataset changes.

## License

By contributing you agree that your contributions are dual-licensed under
`MIT OR Apache-2.0`.
