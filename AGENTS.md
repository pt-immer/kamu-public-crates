# Agent guide — kamu-public-crates

This repository contains nine independently versioned public Rust libraries and
one excluded PostgreSQL extension lane. `CLAUDE.md` and
`.github/copilot-instructions.md` are symlinks to this file; keep this as the
single automation guide. Human-facing orientation belongs in
[`README.md`](README.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Repository map

| Area | Purpose | Build boundary |
| --- | --- | --- |
| `crates/iso3166` | `no_std`, zero-allocation ISO 3166 primitives | Root workspace |
| `crates/logging` | `tracing` setup for native, systemd, wasm, Actix, and OTLP | Root workspace |
| `crates/money-core` | Exact ISO 4217 money, rates, allocation, wire, and driver adapters | Root workspace |
| `crates/snap-*` | Two SNAP BI domain crates plus four Actix/axum adapters | Root workspace |
| `extensions/money-pg` | `kmoney` pgrx extension and YugabyteDB harness | Excluded nested workspace |

The root workspace contains exactly nine publishable crates. Versions and
releases are per crate; each crate's `Cargo.toml` and `CHANGELOG.md` are
authoritative.

The extension lane is structurally excluded from the root `Cargo.toml`. It owns
its Rust 1.96 toolchain, `[patch.crates-io]`, profiles, `Cargo.lock`,
`deny.toml`, and Docker-backed gate. It is `publish = false`, not a tenth public
crate. Do not replace structural exclusion with repeated `--exclude` flags.
Reach the lane through `just pg <recipe>`.

Repository-wide policy remains at the root. In particular, `lint-shell` and
`scrub` cover the extension lane as well as the public workspace.

## Hard invariants

- The public workspace uses Edition 2024 and MSRV 1.94.0. Its primary and
  compile-fail toolchain is pinned to Rust 1.96.0; CI also tests current stable
  and exact MSRV. The extension lane uses Rust 1.96.0 because pgrx 0.19.2
  requires it.
- Never run workspace-wide `--all-features`. `kamu-logging` has mutually
  exclusive native and wasm feature sets; pgrx features select one PostgreSQL
  major. Use the feature matrices in the Justfiles.
- A command such as `--workspace --exclude X` checks nothing when `X` is the
  only member. Prefer a positive package or workspace selection.
- Warnings and `clippy::all` are denied. `kamu-iso3166` also denies
  `clippy::pedantic`.
- `rust-src` is required on both tested toolchains. Compile-fail golden output
  belongs to one toolchain only; never re-bless it after a missing-`rust-src`
  failure.
- `kamu-iso3166` and every `kamu-snap-*` crate forbid unsafe code. The extension
  confines required ABI unsafe syntax to `kamu-money-pg/src/ffi/`; semantic and
  payload code lives under `src/safe/`. Miri covers the safe payload codec, while
  live PostgreSQL and YugabyteDB tests carry the ABI proof.
- `kamu-logging` owns a process-global tracing subscriber. Tests that need fresh
  global state must run in isolated processes; do not call `init()` repeatedly
  to construct error variants.
- Persisted money hashes use
  `kamu_money_core::advanced::stable_hash`. The root source-policy test scans
  every tracked Rust file, including the excluded lane, for
  `DefaultHasher::new`.
- BRI SNAP BI signatures exclude URI queries. The provider vector in
  `crates/snap-crypto/tests/snap_bi_recipes.rs` pins this contract. Adapters pass
  `path()`, not `path_and_query()`, unless a new provider contract and vector
  require otherwise.
- `kamu-snap-crypto` uses `rsa`, affected by RUSTSEC-2023-0071. `deny.toml`
  records the narrow rationale: SNAP BI signs and verifies; it does not decrypt
  attacker-controlled ciphertext. Remove the ignore when a compatible
  constant-time release exists.

## Generated data

`kamu-iso3166` reads
`crates/iso3166/vendor/iso3166-csv/{countries,subdivisions}.csv`, a Git
submodule, during its build. Run `just setup` after cloning. When the submodule
changes, check the pinned cardinalities in
`crates/iso3166/tests/codegen_invariants.rs`.

`kamu-money-core` generates its ISO 4217 register from
`crates/money-core/vendor/list-one.xml`. Change vendored inputs or build scripts,
never generated `OUT_DIR` files. Keep each crate's `NOTICE` and `VENDORED.md`
coherent with its source data.

## Canonical workflow

```sh
just                    # list root recipes
just setup              # submodule, pinned toolchains, targets, and local tools
just doctor             # verify the development environment
just check-all          # fast format + Clippy + test loop
just gate               # complete public-workspace barrier
just ci                 # public gate plus package dry-runs
just pg <recipe>        # enter the excluded lane
just pg gate-offline    # lane checks that need no database
just gate-pg            # developer lane gate; no native YB release build
just gate-all           # public gate plus developer lane gate
just pg gate-pg-release # native YugabyteDB release proof
```

Run `just gate` before pushing public-workspace changes. Run `just gate-all`
before pushing extension changes. The latter takes hours and needs Docker.
Before an extension release, also run `just pg gate-pg-release`; it compiles
the native extension against YugabyteDB and exercises the cluster suites.
YugabyteDB commands serialize the shared default scratch root. Set a unique
`KMONEY_RUN_ROOT` for independent concurrent runs; explicit roots bypass that
default-root lock.

Missing tools or targets fail rather than skip. Tool versions live in
`.config/dev-tools.json`; `just setup` installs the repository-local tools and
cross targets, then runs `just doctor`. ShellCheck remains an operating-system
package; setup prints the required version when it is absent.
`VERBOSE=1` exposes full output behind compact aggregate recipes.

Both container images compile their dependencies in a layer of their own, keyed
on the manifests, so editing lane source recompiles `kamu-money-pg` and nothing
beneath it. `KMONEY_BUILD_CACHE_DIR` makes `test-matrix.sh` and the `yb-build`
recipe export and restore those layers through `docker buildx`, scoped per
PostgreSQL major and once for YugabyteDB; CI sets it, and locally it stays unset
because the daemon already holds the layers. The YugabyteDB export names the
`deps` build target: `mode=max` over the whole graph would also export the
package step's `target/`, which nothing restores.

A layer cache is exactly what a release proof must be able to bypass.
`KMONEY_CACHE_ID` is expanded inside the dependency compile, so the unique value
`gate-pg-release` derives is a guaranteed cache miss and therefore a genuine
from-scratch build. An ARG that were declared and never referenced would scope
nothing and say so nowhere, which is why `hygiene/tests/packaging.rs` asserts
the reference rather than the declaration.

Both container images must start from the exact toolchain
`extensions/money-pg/rust-toolchain.toml` names. A series tag such as
`rust:1.96` floats to a newer patch; rustup then honours the pin by downloading
a second toolchain inside every container, on every run, without ever producing
a wrong answer. `hygiene/tests/pins.rs` holds that agreement, and the pgrx one.

Granular root checks:

```sh
just lint-all           # Rust, Markdown, TOML, spelling, shell, scrub
just test-all           # workspace and per-crate feature matrices
just cov-all            # enforced coverage floors
just check <crate>      # one crate, without the workspace sweep
just test-fast          # workspace nextest plus doctests
```

New recipes use the `<area>-<verb>` / `*-all` naming scheme. Aggregates compose
granular recipes; CI should call the same granular recipes rather than duplicate
their commands.

## Test policy

- `cargo-nextest` is the ordinary test runner in recipes, coverage, and CI.
  `.config/nextest.toml` is its single configuration. Retries remain disabled.
- Nextest does not run doctests. Complete ordinary-test aggregates must own an
  explicit `cargo test --doc`; coverage measurements intentionally exclude
  doctests.
- Container-backed tests are bounded by nextest test groups, not one-off command
  flags.
- The root gate stays Docker-free. Docker-dependent coverage belongs to CI or
  the extension gate and must be named as non-coverage when omitted.
- Coverage floors are `kamu-iso3166` 98%, `kamu-logging` 88%,
  `kamu-money-core` 80%, `kamu-snap-crypto` 70%, and
  `kamu-snap-response` 85%. The four thin framework adapters are
  behavior/compile-tested but have no percentage floor.
- A floor is set only after measurement. New behavior lands with tests.
- Markdown fences need languages, tables must lint, and Taplo owns TOML
  formatting.

## CI structure

`.github/workflows/on-pr-synced.yml` runs for pull requests and pushes to
`main`. `scripts/ci_paths.py` classifies every changed path and fails when a
repository surface has no owner. `just test-repo-policy` proves every tracked
path remains classified.

Compiled package inputs under `crates/money-core` also select the dependent
`moneypg` lane; crate documentation alone does not. A path-filtered job must not
depend on a job with a narrower path condition unless it handles skipped
dependencies explicitly. The workflow policy test simulates this skip cascade
for every tracked path.

Heavy jobs use job-level conditions. Do not add workflow-level `paths:` filters:
they can leave required checks pending. The six SNAP crates share one change
class because their dependency graph requires coordinated testing.

`ci-success` is the sole required branch check. It gathers every job through
`re-actors/alls-green`; keep its `needs` and allowed-skip list complete whenever
jobs change. A recipe added only to a local aggregate is not CI coverage—verify
that some CI job reaches it.

Third-party actions use full commit IDs with a readable release label in a
comment. Workflow outputs and environment variables use underscores, never
hyphens; GitHub expressions parse a hyphen as subtraction.

Workflows that receive the crates.io token target the `crates-io` environment,
which scopes the token. It carries **no reviewer rule**: publishing is gated by
creating the GitHub Release, not by a second approval.

Pull requests into `main` are gated and reviewed. The repository owner's
`--admin` merge is the only sanctioned bypass — do not relax the rule because
merges routinely use it.

`default_workflow_permissions` is `read`. Every workflow declares the
permissions it needs.

Ordinary extension container tests receive Cargo's normalized
`kamu-money-core` package through a named Docker context. Release proof sets
`KMONEY_USE_LOCAL_CORE=0`, so the plain version dependency must resolve from the
registry. Do not widen the primary Docker context to the repository root.

## Commits

Use lowercase, imperative Conventional Commit subjects:
`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, or `chore:`, optionally scoped.
Name branches `<type>/tdkc-<n>-<slug>`.

Every commit is GPG-signed and carries its lowercase JIRA ticket as a standalone
paragraph:

```text
chore(deps): refresh workspace dependencies

Update requirements within the Rust 1.94 compatibility range.

tdkc-1
```

Tickets use the `tdkc-` prefix. History before 2026-08 carries the earlier
`kec-` prefix; leave those commit messages as they are.

Verify signed history with `git log --show-signature`.

## Releases

Public crates release independently:

1. Update the crate's version and `CHANGELOG.md`.
2. Merge the change to `main`.
3. Create `<crate>-vX.Y.Z` from `main`.
4. Let `on-release-published.yml` verify ancestry, version, dependency
   availability, and non-republication before publishing that crate.

A lockfile-only dependency refresh needs no crate version bump. A crate source or
manifest change does. Compare manifest versions with crates.io before deciding
which releases remain pending.

SNAP crates publish in dependency order:

1. `kamu-snap-crypto`
2. `kamu-snap-response`
3. the four `kamu-snap-{crypto,response}-{actix,axum}` adapters

Wait for the crates.io index between tiers.

The release workflow recognizes `kamu-money-pg-vX.Y.Z`, verifies its manifest,
and stops before crates.io. The extension is a `cdylib` whose graph depends on a
root Cargo patch and is not publishable as a crate.

The first publication of a new crate must add
`github:pt-immer:rust-devs` as an owner. The release workflow attempts this; the
manual `add-crate-owner.yml` workflow backfills it.

## Keep this guide current

Update this file in the same change when repository structure, tools, recipes,
gates, CI ownership, release mechanics, or standing invariants change. Repeated
workflow behavior that contradicts this guide is a defect in the guide.

Keep durable history in changelogs. Keep current contracts in code, tests,
crate-level design documents, and runbooks. Do not turn this guide into an
incident diary.

## Licensing

Source is dual-licensed `MIT OR Apache-2.0`. `kamu-iso3166` additionally embeds
ISO 3166 data under CC BY-SA 4.0; see its `NOTICE` and `VENDORED.md`.
`kamu-money-core` carries separate attribution for its ISO 4217 register. The
SNAP crates were relicensed from MIT to `MIT OR Apache-2.0` on import.
