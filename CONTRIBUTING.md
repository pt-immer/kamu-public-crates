# Contributing to kamu-public-crates

Thank you for contributing. The root Cargo workspace contains public
libraries that version and release independently. The PostgreSQL extension under
The PostgreSQL extension is maintained in the separate
[kamu-money-pg repository](https://github.com/pt-immer/kamu-money-pg).

## Setup

```sh
git clone --recurse-submodules https://github.com/pt-immer/kamu-public-crates.git
cd kamu-public-crates
cargo run -q -p repo-policy --bin dev-env -- setup
export PATH="$PATH:$PWD/.tools/bin"
just doctor
```

The recursive clone matters: `kamu-iso3166` generates lookup tables from its
vendored Git submodule. The bootstrap command works before `just` exists,
installs the versions in `.config/dev-tools.json`, and uses `npm ci`. The
export appends, matching the order the `Justfile` exports to every recipe:
those pinned binaries answer where the host provides nothing. Prepending
inverts that for the shell you are in, and nothing reports it. ShellCheck is an
operating-system package; `just doctor` names the version to install when it is
missing.

Your machine's own tools answer first, and setup fills only what they do not
provide. Why a developer machine and a CI runner provision differently, and what
a pin means in each, is in
[`docs/TOOLCHAIN-REALMS.md`](docs/TOOLCHAIN-REALMS.md).

## Development loop

```sh
just check-all  # fast format, Clippy, and test signal
just gate       # complete barrier for the public crates
just ci         # gate plus publish dry-runs
```

Run `just gate` before pushing a public-workspace change. It covers formatting,
Clippy, tests and feature permutations, the exact MSRV, documentation,
cross-target builds, dependency policy, spelling, repository hygiene, and
enforced coverage.

`just check-all` is intentionally smaller. It is useful while editing, but is
not a release or pre-push barrier.

### Test conventions

Ordinary tests run with
[cargo-nextest](https://nexte.st/), configured in
`.config/nextest.toml`. Nextest does not run doctests, so complete ordinary-test
aggregates also run `cargo test --doc`. Coverage recipes intentionally exclude
doctests from their measurements. Preserve an explicit doctest owner when
adding or splitting test aggregates.

Do not use workspace-wide `--all-features`. `kamu-logging` has mutually
exclusive native and wasm features. The Justfile holds the supported matrices.

Each crate that carries a line-coverage floor states it in its own `cov-*`
recipe and nowhere else, beside the reason it sits where it does; `just cov-all`
gathers them and prints each measurement against its floor. A floor is set from
a measurement, never from a target. The thin Actix/axum adapters carry none;
they are behavior- and compile-tested instead.

## Commits

Use an imperative, lowercase
[Conventional Commit](https://www.conventionalcommits.org/) subject, optionally
scoped:

```text
feat(iso3166): add Alpha2::iter()
```

Work uses lowercase `tdkc-` JIRA tickets. Name branches
`<type>/tdkc-<n>-<slug>`. Every commit must be GPG-signed and place its ticket in
a standalone paragraph before any trailer block:

```text
chore(deps): refresh workspace dependencies

Update requirements within the declared compatibility range.

tdkc-1
```

History before 2026-08 carries the earlier `kec-` prefix; leave those commit
messages as they are.

## Releasing a crate

Public crates release independently:

1. Update the crate's version in `Cargo.toml`.
2. Update that crate's `CHANGELOG.md`.
3. Merge the change to `main`.
4. Create a GitHub Release named `<crate>-vX.Y.Z` from `main`.

`on-release-published.yml` verifies the tag, manifest version, main ancestry,
dependency availability, and crates.io state before the protected `crates-io`
environment approves publishing exactly one crate. A lockfile-only refresh does
not require a version bump; a crate source or manifest change does.

The PostgreSQL extension consumes published core releases independently.
Its dependency-update PRs own extension integration validation.

The SNAP family must publish in dependency order:

1. `kamu-snap-crypto`
2. `kamu-snap-response`
3. `kamu-snap-{crypto,response}-{actix,axum}`

Wait for the crates.io index between tiers. Cargo cannot package a crate while
an in-workspace dependency—even an optional one—is unavailable from the
registry.

## Updating standards data

- ISO 3166: follow
  [`crates/iso3166/VENDORED.md`](crates/iso3166/VENDORED.md), then update
  cardinality assertions if the consumed CSV rows changed.
- ISO 4217: follow
  [`crates/money-core/VENDORED.md`](crates/money-core/VENDORED.md). The build
  validates the vendored register and generates the Rust table.

Never edit generated `OUT_DIR` tables directly.

## License

Contributions are accepted under `MIT OR Apache-2.0`. Vendored standards data
retains the terms and attribution documented by its owning crate.
