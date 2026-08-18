# Agent guide — kamu-public-crates

This repository contains independently versioned public Rust libraries, an
unpublished repository-policy crate, and one excluded PostgreSQL extension lane.
`CLAUDE.md` and `.github/copilot-instructions.md` are symlinks to this file; keep
this as the single automation guide. Human-facing orientation belongs in
[`README.md`](README.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Repository map

| Area | Purpose | Build boundary |
| --- | --- | --- |
| `crates/iso3166` | `no_std`, zero-allocation ISO 3166 primitives | Root workspace |
| `crates/logging` | `tracing` setup for native, systemd, wasm, Actix, and OTLP | Root workspace |
| `crates/money-core` | Exact ISO 4217 money, rates, allocation, wire, and driver adapters | Root workspace |
| `crates/snap-*` | SNAP BI domain crates plus their Actix/axum adapters | Root workspace |
| `extensions/money-pg` | `kmoney` pgrx extension and YugabyteDB harness | Excluded nested workspace |
| `tools/repo-policy` | Decoders for the repository's own artifacts, and the checks that read them | Root workspace, `publish = false` |

Every root workspace member is published except `tools/repo-policy`, which is
`publish = false`. Versions and releases are per crate; each crate's
`Cargo.toml` and `CHANGELOG.md` are authoritative.

The extension lane is structurally excluded from the root `Cargo.toml`. It owns
its own Rust toolchain, `[patch.crates-io]`, profiles, `Cargo.lock`,
`deny.toml`, and Docker-backed gate. It is `publish = false`, not another public
crate. Do not replace structural exclusion with repeated `--exclude` flags.
Reach the lane through `just pg <recipe>`.

Repository-wide policy lives at the root. In particular, `lint-shell` and
`scrub` cover the extension lane as well as the public workspace.

## Hard invariants

- The public workspace uses Edition 2024. `.config/dev-tools.json` is the one file
  CI reads its versions from. It states three Rust versions: `rust.msrv` is the
  floor the published manifests declare and CI tests exactly, `rust.primary` is
  the toolchain that pins compile-fail goldens, and `rust.lane` is the excluded
  extension lane's channel. Each is a view of a file some tool honours and the
  manifest cannot — `rust-toolchain.toml` for the two channels, `Cargo.toml` for
  the floor — and `tools/repo-policy` holds them equal. The rest are tool pins,
  and every `tool:` request indexes the entry for the tool it names rather than
  restating a version.

  Each tool section is an object keyed by the name that tool is requested and
  installed by — `install-action`'s spelling for the ones CI installs, the npm
  package name for `node_tools`, which npm installs and no workflow requests.
  Keying is what makes a pin reachable at all: an Actions expression can index a
  value by name but cannot search a list for the entry whose name matches. That
  key is also what cargo installs and what a recipe runs, so an entry states
  `crate`, `package`, `binary` or `version_args` only where one differs from it
  or from the default.

  `.github/actions/read-dev-tools` publishes the manifest itself, once, and a job
  republishes that one output; each site indexes the pin it needs out of it. A
  pin added to the manifest is therefore reachable without an output and a
  republish being added to carry it, and every path a workflow indexes is checked
  to exist — an unresolvable one is not an error in Actions, it is the empty
  string, and a job handed one installs whatever the runner already had. No file
  Actions executes states a three-component version literal outside a comment, in
  either YAML spelling and on any line: not a toolchain selection, not a `tool:`
  request, not a cache key, not a `run:` line. Three is what every version this
  repository pins has, and what a dotted address is not. A version written with
  any other number of components — a PostgreSQL major, an image series, the
  four-component YugabyteDB image tag — reads the same as an ordinary number or
  an address, so that scan does not claim it, and each is bound by whatever else
  names it or by nothing.

  Toolchain components and targets named in a job sit outside the one-home rule.
  `rust.primary_components` and `rust.primary_targets` state what `just setup`
  installs on a developer machine, held equal to `rust-toolchain.toml`; a
  `components:` or `targets:` line in a job states what that job needs, which is
  a different claim even where the two name the same string. Collapsing them
  would make the manifest the home for a requirement each job owns, and would
  leave the ones no developer machine installs — `llvm-tools-preview`, `miri` —
  with nowhere to live.

  `test_dev_environment.py` binds the literals in `clippy.toml`, the
  `Justfile` and `README.md`. Toolchain literals matter more than the floor they
  restate: `rustup run <version>` addresses a toolchain by name, so one that
  outlives a bump either resolves to a stale install or does not resolve at all.
  The extension lane pins its own toolchain because the pgrx it is patched to
  requires that one, and binds it in its hygiene crate.
  Its CI jobs install `rust.lane` rather than `rust.primary`, and
  `test_workflows.py` tells the two apart by whether a job invokes a root recipe
  that cds into the lane, a set it derives from the `Justfile` rather than lists
  — `just pg` is not the only way in, and `gate-all` composes `gate-pg`.
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
  compiling the published crate while reporting nothing. Re-lock with
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
- The lane patches `pgrx` to a fork for every target, not only YugabyteDB, and
  nothing here can verify that fork stays equivalent to upstream. A workspace has
  one `[patch.crates-io]` table and one lockfile, and `kamu-money-pg` declares
  `yb-pg15 = ["pgrx/yb-pg15"]`, which Cargo validates at resolution whether or
  not the feature is enabled — so a stock `pgrx` fails to resolve everywhere. The
  PostgreSQL 15–18 matrix therefore builds the fork with the feature off and
  relies on it compiling to upstream. Measured at `v0.19.2-yb.1`: five files
  differ, no upstream line is removed or modified, every addition is behind
  `#[cfg(feature = "yb-pg15")]`. A revision that changed an upstream line instead
  would be silent, because the fork keeps its version compatible with
  `cargo-pgrx` and no version check can see content. Re-measure before accepting
  a new tag, and delete this entry once the fork's own CI proves it
  (`pt-immer/pgrx-yugabytedb#1`).

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
cross targets, then runs `just doctor`. ShellCheck is an operating-system
package; setup prints the required version when it is absent.
`VERBOSE=1` exposes full output behind compact aggregate recipes.

The Justfile exports `.tools/bin` and `node_modules/.bin` after `PATH`, so a
recipe runs the host's own tool where the host provides one and the
repository-local copy where it does not. `just doctor` resolves in that same
order. Under **Repository tools**, where setup is what installs them, the marker
names the copy it found: `✓` repository-local, `•` host. Elsewhere `✓` means
satisfied, because nothing in those sections is setup's to install. `✗` is
always absent, unreadable, or not answering its pin.

A pinned tool version is a floor unless its entry states why it must be exact,
and the row prints the comparison it made. A tool whose output is itself a gate
verdict is pinned exactly, because a copy above the floor passes locally and
fails in CI. Rust toolchains stay exact, because `rustup run <version>`
addresses one by name — an identity, not a floor.

A developer machine and a CI runner provision differently on purpose, and
[`docs/TOOLCHAIN-REALMS.md`](docs/TOOLCHAIN-REALMS.md) is the one place that
describes both. It also carries the consequence of the search order: the host's
copy answers before the repository-local one, so a host tool that misses its pin
shadows anything setup installs, and setup refuses to install beneath it.

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

CI sets it for the YugabyteDB job only, because the PostgreSQL jobs run in
parallel: an uncached major sets their pace whether or not its siblings are
cached, while the YugabyteDB job is both the longest and alone. Whether a size
cap also binds this choice is UNKNOWN — the arithmetic once recorded here
assumed a 10 GB repository cache, and active caches exceed that. The cap is the
half that is missing: `actions/cache/usage` reports consumption only, and the
limit comes from the organization's cache usage policy or the repository's
Actions settings. Re-derive both before relying on a size reason. A target that
is cached and then evicted is
worse than one never cached: each run pays to download a stale near-miss and
rebuilds the layer anyway. The PostgreSQL images build their dependencies in a
layer; only the export is dropped. Exporting needs the docker-container buildx
driver, which the selected builder is not by default;
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
`extensions/money-pg/rust-toolchain.toml` names. A base image named by series
rather than by patch floats to a newer patch; rustup then honours the pin by
downloading a second toolchain inside every container, on every run, without
ever producing a wrong answer. `hygiene/tests/pins.rs` holds that agreement, and
the pgrx one.

Granular root checks:

```sh
just lint-all           # Rust, Markdown, TOML, spelling, shell, scrub
just test-all           # workspace and per-crate feature matrices
just cov-all            # enforced coverage floors
just check <crate>      # one crate, without the workspace sweep
just test-fast          # workspace nextest plus doctests
just test-scripts       # CI path ownership and workflow policy
just test-policy        # the pinned versions and what Actions may run
just pg selftest-all    # the compiler-free lane negative controls; CI runs this
just pg doc-gate-selftest # the doc gate's controls; needs a populated PGRX_HOME
just pg core-relock     # re-lock kamu-money-core with the lane patch active
```

Every negative control must be reached by a required check directly. A control
reachable only through `gate-offline` is covered by nothing, and one that never
runs cannot be told from one that cannot fail.

`selftest-all` gathers the controls that need no compiler, and the CI job that
runs it installs no PostgreSQL. `doc-gate-selftest` plants broken intra-doc links
and runs `doc-pg`, so it needs the toolchain and a populated `PGRX_HOME`; it has
its own recipe and its own step in the job that already has both. Adding it to
`selftest-all` moves it to a job where `cargo doc` dies on `$PGRX_HOME`.

New recipes use the `<area>-<verb>` / `*-all` naming scheme. Aggregates compose
granular recipes; CI should call the same granular recipes rather than duplicate
their commands.

## Test policy

- `cargo-nextest` is the ordinary test runner in recipes, coverage, and CI.
  `.config/nextest.toml` is its single configuration. Retries are disabled.
- Nextest does not run doctests. Complete ordinary-test aggregates must own an
  explicit `cargo test --doc`; coverage measurements intentionally exclude
  doctests.
- Container-backed tests are bounded by nextest test groups, not one-off command
  flags.
- The root gate stays Docker-free. Docker-dependent coverage belongs to CI or
  the extension gate and must be named as non-coverage when omitted.
- Each line-coverage floor lives in its `cov-*` recipe and nowhere else, beside
  the reason it sits where it does. The thin framework adapters carry no
  percentage floor; they are behavior- and compile-tested instead.
- A floor is set only after measurement. New behavior lands with tests.
- Markdown fences need languages, tables must lint, and Taplo owns TOML
  formatting.

## CI structure

One gate per event, and a workflow named for the event it answers.
`publish-builder-image.yml` shares the push trigger and gates nothing; it
publishes an artifact when the inputs that decide it change.
`.github/workflows/on-pr-synced.yml` answers pull
requests, and `workflow_dispatch` for a run against a branch on demand, which
diffs the empty tree so every job runs. It has no push trigger: the ruleset
requires an up-to-date branch, squash-only merges and linear history, so a merge
lands the tree the run already certified. Nothing seeds the cache that leaves
behind, because that pool is keyed per job.

The administrative override and a direct push land no such certificate, so
`on-main-pushed.yml` requires one: the landing tree must be the tree a green
`ci-success` covered on the pull request head. It is also what the `CI` badges
read, because a `pull_request` run is never attributed to `main`.

`on-release-published.yml` compiles and tests the extension against the
`kamu-money-core` it just published, across every supported PostgreSQL major.
It does not reach the YugabyteDB path, which stays with
`just pg gate-pg-release`. It runs after the version is immutable, so a failure
is answered by yanking, not by blocking a merge.

`tools/repo-policy`'s path classifier classifies every changed path and fails when a repository
surface has no owner. `just test-scripts` proves every tracked path remains
classified.

Why working on one crate runs another's jobs is answered by `DERIVED_CLASSES` in
`tools/repo-policy/src/ci_paths.rs`, which carries every fan-out edge with its reason. The
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
comment. Workflow outputs and environment variables use underscores. An
environment name becomes a shell variable, where a hyphen cannot appear in one;
outputs take the same spelling so a reference never changes form between them.

Workflows that receive the crates.io token target the `crates-io` environment,
which scopes the token. It carries **no reviewer rule**: publishing is gated by
creating the GitHub Release, not by a second approval.

Pull requests into `main` are gated and reviewed. Branch protection permits an
administrative override; exercising it does not relax the rule, and a merge that
used one is not precedent for the next.

`default_workflow_permissions` is `read`. Every workflow declares the
permissions it needs. `publish-builder-image.yml` is the one that writes a
package, and it publishes the lane's reusable pgrx build environment from the
default branch. Its tag is derived from the inputs that decide the image, so it
builds when one of them changes and at no other time; the base image is pinned
by digest so an upstream rebuild is one of those changes.
[`docs/TOOLCHAIN-REALMS.md`](docs/TOOLCHAIN-REALMS.md) carries the model.

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

Update requirements within the declared compatibility range.

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
3. the `kamu-snap-{crypto,response}-{actix,axum}` adapters

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
