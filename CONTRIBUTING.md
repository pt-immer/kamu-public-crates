# Contributing to kamu-public-crates

Thank you for contributing. The root Cargo workspace contains nine public
libraries that version and release independently. The PostgreSQL extension under
`extensions/money-pg` is a separate, excluded workspace.

## Setup

```sh
git clone --recurse-submodules https://github.com/pt-immer/kamu-public-crates.git
cd kamu-public-crates
python3 scripts/dev_environment.py setup
export PATH="$PWD/.tools/bin:$PATH"
just doctor
```

The recursive clone matters: `kamu-iso3166` generates lookup tables from its
vendored Git submodule. The bootstrap command works before `just` exists,
installs the versions in `.config/dev-tools.json`, and uses `npm ci`. Exporting
the local tool directory makes those pinned binaries available in the current
shell. ShellCheck 0.11.0 remains an operating-system package.

## Development loop

```sh
just check-all  # fast format, Clippy, and test signal
just gate       # complete barrier for the nine public crates
just ci         # gate plus publish dry-runs
```

Run `just gate` before pushing a public-workspace change. It covers formatting,
Clippy, tests and feature permutations, exact MSRV 1.94.0, documentation,
cross-target builds, dependency policy, spelling, repository hygiene, and
enforced coverage.

`just check-all` is intentionally smaller. It is useful while editing, but is
not a release or pre-push barrier.

### Extension changes

Enter the excluded lane through the root passthrough:

```sh
just pg             # list lane recipes
just pg gate-offline
just gate-all       # root gate plus the developer lane gate
just pg gate-pg-release  # native YugabyteDB correctness proof
just pg test-yb-deployment  # cluster, read replica, concurrency, restore
```

`just gate-all` needs Docker and can take hours. Run it before pushing a change
under `extensions/money-pg`. Run `just pg gate-pg-release` before an extension
release; it includes the from-source native YugabyteDB build, the byte-exact A/B
against upstream PostgreSQL 15 and the ported case suite, which the ordinary
development gate omits. The deployment suites are separate: see
`just pg test-yb-deployment`.

### Test conventions

Ordinary tests run with
[cargo-nextest](https://nexte.st/), configured in
`.config/nextest.toml`. Nextest does not run doctests, so complete ordinary-test
aggregates also run `cargo test --doc`. Coverage recipes intentionally exclude
doctests from their measurements. Preserve an explicit doctest owner when
adding or splitting test aggregates.

Do not use workspace-wide `--all-features`. `kamu-logging` has mutually
exclusive native and wasm features, and pgrx features select one PostgreSQL
major. The Justfiles hold the supported matrices.

Current line-coverage floors are:

| Crate | Floor |
| --- | ---: |
| `kamu-iso3166` | 98% |
| `kamu-logging` | 88% |
| `kamu-money-core` | 86% |
| `kamu-snap-crypto` | 70% |
| `kamu-snap-response` | 85% |

The four thin Actix/axum adapters are behavior- and compile-tested without a
percentage floor.

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

Update requirements within the Rust 1.94 compatibility range.

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

The SNAP family must publish in dependency order:

1. `kamu-snap-crypto`
2. `kamu-snap-response`
3. `kamu-snap-{crypto,response}-{actix,axum}`

Wait for the crates.io index between tiers. Cargo cannot package a crate while
an in-workspace dependency—even an optional one—is unavailable from the
registry.

The excluded `kamu-money-pg` lane may have a versioned GitHub Release, but its
workflow stops before crates.io. It is a native extension, not a publishable Rust
library.

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
