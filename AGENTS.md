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

- The public workspace uses Edition 2024. `.config/dev-tools.json` owns both
  Rust versions: `rust.msrv` is the floor the root manifest declares and CI
  tests exactly, and `rust.primary` is the toolchain that pins compile-fail
  goldens. `test_dev_environment.py` binds every literal that names a Rust
  version in the two manifests, the CI toolchain matrix, `clippy.toml`, the
  `Justfile` and `README.md` — a literal that is neither version fails. Toolchain
  literals matter more than the floor they restate: `rustup run <version>`
  addresses a toolchain by name, so one that outlives a bump either resolves to
  a stale install or does not resolve at all. The extension lane pins its own
  toolchain because pgrx 0.19.2 requires it, and binds it in its hygiene crate.
  Its CI jobs install that toolchain rather than `rust.primary`, and
  `test_workflows.py` tells the two apart by whether a job runs `just pg`.
  `rust-toolchain.toml` wins over whatever a job installed, so a lane job given
  the public workspace's toolchain still compiles with the lane's — after rustup
  downloads it, inside every job, on every run, with no wrong answer to notice
  it by. The lane's MSRV is a third value again: `clippy.toml` and the lane
  manifest state it, `pins.rs` holds those two equal, and against the pinned
  channel the tie is an inequality, because deriving a floor from the compiler
  in use leaves `clippy::incompatible_msrv` comparing it against itself.
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
- The extension lane's `Cargo.lock` must record `kamu-money-core` at the version
  `crates/money-core` carries. A `[patch.crates-io]` entry offering any other
  version is ignored rather than refused, so the container suites go on
  compiling the published crate while reporting nothing; that is how the lane
  spent a release cycle testing 0.1.1 against a 0.1.2 tree. Re-lock with
  `just pg core-relock` — a bare `cargo update` in the lane re-locks it to the
  registry. The entry's form is not fixed: Cargo records whichever resolution it
  last performed, so a patched run writes a path and an unpatched one writes a
  registry source. `scripts/assert-core-resolution.sh` re-checks the resolved
  graph inside every image, in both directions and at the expected version,
  because the release proof needs the opposite answer from an ordinary suite and
  a patched lockfile pins no version for it.
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
just pg gate-pg-release # native YugabyteDB correctness proof
just pg test-yb-deployment # cluster, read replica, concurrency, dump and restore
```

Run `just gate` before pushing public-workspace changes. Run `just gate-all`
before pushing extension changes. The latter takes hours and needs Docker.
Before an extension release, also run `just pg gate-pg-release`; it compiles
the native extension against YugabyteDB, proves it byte-exact against upstream
PostgreSQL 15, and runs the ported case suite. It deliberately stops there:
replication, tablet placement, read replicas and dump/restore are YugabyteDB's
behaviour rather than the extension's, and live in `just pg test-yb-deployment`
for whoever adopts a new image or changes how the extension is deployed.
YugabyteDB commands serialize the shared default scratch root. Set a unique
`KMONEY_RUN_ROOT` for independent concurrent runs; explicit roots bypass that
default-root lock.

Missing tools or targets fail rather than skip. Tool versions live in
`.config/dev-tools.json`; `just setup` installs the repository-local tools and
cross targets, then runs `just doctor`. ShellCheck remains an operating-system
package; setup prints the required version when it is absent.
`VERBOSE=1` exposes full output behind compact aggregate recipes.

The Justfile exports `.tools/bin` and `node_modules/.bin` ahead of `PATH`, so a
recipe runs the repository-local tool where setup installed one and the system
copy where it did not. `just doctor` resolves in that same order. Under
**Repository tools**, where setup is what installs them, the marker names the
copy it found: `✓` repository-local, `•` system. Elsewhere `✓` means satisfied,
because nothing in those sections is setup's to install. `✗` is always absent,
unreadable, or below its pin.

Pinned tool versions are floors, not equalities. Any tool at or above its
`.config/dev-tools.json` version passes, and the row prints the comparison.
`just setup` still installs the exact pin and every workflow still installs it,
so a tool above the floor is running something CI is not: for `taplo`, `typos`,
`markdownlint-cli2` and `shellcheck`, whose output is itself a gate verdict, that
can pass locally and fail in CI. Rust toolchains stay exact, because
`rustup run <version>` addresses one by name — an identity, not a floor.

`just pg doctor` follows the same rendering contract, with one marker of its
own: `!` is an advisory warning, which never affects its exit code. It is a
distinct glyph because `•` already means a tool that passed from outside the
repository, and a warning is not a pass.

Both doctors color an interactive stdout only and honor `NO_COLOR`. `setup`,
`doctor` and the `pg` passthrough carry `[no-exit-message]`, so a failure is the
script's own report; the exit status still travels.

Both container images compile their dependencies in a layer of their own, keyed
on the manifests, so editing lane source recompiles `kamu-money-pg` and nothing
beneath it. `KMONEY_BUILD_CACHE_DIR` makes `test-matrix.sh` and the `yb-build`
recipe export and restore those layers through `docker buildx`; locally it stays
unset because the daemon already holds the layers.

CI sets it for the YugabyteDB job only. Each cached target costs about 1.6 GiB
per manifest state against a 10 GB repository cache, and a branch plus a pull
request touching a manifest are two live states, so caching all five exceeds the
limit; saves are then refused and each run rebuilds the layer whose stale
near-miss it just paid to download, which is worse than not caching. One target
fits, and YugabyteDB is the one worth it: the PostgreSQL jobs run in parallel, so
an uncached major sets their pace whether or not its siblings are cached, while
the YugabyteDB job is both the longest and alone. The PostgreSQL images still
build their dependencies in a layer; only the export is dropped. Exporting needs
the
docker-container buildx driver, which the selected builder is not by default;
`scripts/require-cache-exporter.sh` refuses rather than letting the build abort
part-way. The YugabyteDB export names the `deps` build target: `mode=max` over
the whole graph would also export the package step's `target/`, which nothing
restores.

The normalized `kamu-money-core` package is copied into both dependency layers,
so `crates/money-core` and the root lockfile are inputs to them and belong in
the CI cache keys. Omitting them is not a slow build but a silently stale one:
an exact key hit makes `actions/cache` skip its post-job save, so the layer
buildx correctly rebuilt is discarded and every later run rebuilds it too. For
the same reason `docker-core-context.sh` deletes `.cargo_vcs_info.json` from
the packaged directory — it carries the HEAD sha1, which would give that layer
a cache key that can never repeat.

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
just pg selftest-all    # every lane negative control; CI runs this recipe
just pg core-relock     # re-lock kamu-money-core with the lane patch active
```

Negative controls belong to `selftest-all`, and a CI job runs that recipe
directly. A control reachable only through `gate-offline` is not covered by any
required check, and one that never runs cannot be told from one that cannot
fail.

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
- Each line-coverage floor lives in its `cov-*` recipe and nowhere else, beside
  the reason it sits where it does. Five crates have one; the four thin
  framework adapters are behavior/compile-tested without a percentage floor.
- A floor is set only after measurement. New behavior lands with tests.
- Markdown fences need languages, tables must lint, and Taplo owns TOML
  formatting.

## CI structure

`.github/workflows/on-pr-synced.yml` runs for pull requests and pushes to
`main`. `scripts/ci_paths.py` classifies every changed path and fails when a
repository surface has no owner. `just test-repo-policy` proves every tracked
path remains classified.

Why working on one crate runs another's jobs is answered by `DERIVED_CLASSES` in
`scripts/ci_paths.py`, which carries every fan-out edge with its reason. The
reason is a required field and the map is checked against an independently
written expectation, so an edge cannot be added, widened or narrowed silently.
Change the map, not this paragraph.

A path-filtered job must not depend on a job with a narrower path condition
unless it handles skipped dependencies explicitly. The workflow policy test
simulates this skip cascade for every tracked path.

Heavy jobs use job-level conditions. Do not add workflow-level `paths:` filters:
they can leave required checks pending.

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
