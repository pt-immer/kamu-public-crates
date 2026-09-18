# Agent guide — kamu-public-crates

This repository contains independently versioned public Rust libraries and an
unpublished repository-policy crate. `CLAUDE.md` and
`.github/copilot-instructions.md` are symlinks to this file. Human-facing
orientation belongs in [README.md](README.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

## Repository map

| Area | Purpose |
| --- | --- |
| `crates/iso3166` | `no_std`, zero-allocation ISO 3166 primitives |
| `crates/logging` | tracing for native, systemd, wasm, Actix and OTLP |
| `crates/money-core` | Exact ISO 4217 money, rates, allocation, wire and driver adapters |
| `crates/snap-*` | SNAP BI domain crates and Actix/axum adapters |
| `tools/repo-policy` | Repository artifact decoders and policy checks; `publish = false` |

Every root workspace member is published except `tools/repo-policy`, which is
`publish = false`. Versions
and releases are per crate; each crate's Cargo.toml and CHANGELOG.md are
authoritative. The Cloudflare Worker example is a separate excluded workspace.

The PostgreSQL/YugabyteDB extension lives in
[kamu-money-pg](https://github.com/pt-immer/kamu-money-pg). It owns its builds,
CI, toolchain, builder images and release proofs, and consumes published
money-core versions through crates.io. This repository does not trigger its
builds. Retired extension paths remain classified for extraction diffs; a
tracked-file check prohibits their reintroduction.

## Hard invariants

- Public crates use Edition 2024. `.config/dev-tools.json` is the only file CI
  reads tool versions from. `rust.msrv` is the declared floor CI tests exactly;
  `rust.primary` is the channel that pins compile-fail goldens. Policy checks
  bind them to Cargo.toml and rust-toolchain.toml.
- Each tool section is keyed by the name used to request and install the tool.
  Entries state crate, package, binary or version arguments only where those
  differ from the default. Every workflow pin reference must resolve; a missing
  Actions path silently becomes an empty string.
- `.github/actions/read-dev-tools` publishes the manifest once. Jobs republish
  that output and index the tool they need. No Actions-executed YAML states a
  three-component version outside a comment, including run lines and cache keys.
- Developer toolchain components and targets are bound to rust-toolchain.toml.
  A job's components and targets are its own requirements; do not collapse them
  into developer-machine requirements. Miri and llvm-tools-preview are examples.
- Never run workspace-wide `--all-features`: logging has mutually exclusive
  native and wasm feature sets. Use the Justfile feature matrices.
- Prefer positive package/workspace selections. Excluding the sole workspace
  member checks nothing.
- Warnings and `clippy::all` are denied. ISO 3166 also denies `clippy::pedantic`.
- rust-src is required on both tested toolchains. Never re-bless compile-fail
  goldens after a missing-rust-src failure.
- ISO 3166 and every SNAP crate forbid unsafe code.
- Logging owns a process-global subscriber. Tests needing fresh state run in
  isolated processes; do not call init repeatedly to construct error variants.
- Persisted money hashes use `kamu_money_core::advanced::stable_hash`.
  Repository policy parses every tracked Rust file and rejects DefaultHasher
  construction.
- BRI SNAP BI signatures exclude URI queries. The provider vector in
  `crates/snap-crypto/tests/snap_bi_recipes.rs` pins this. Framework adapters
  derive the path themselves; the lower-level verify_request cannot enforce
  that a caller supplied only a path.
- SNAP crypto's rsa dependency is affected by RUSTSEC-2023-0071. deny.toml records
  the narrow signing/verification rationale; remove the ignore when a compatible
  constant-time release exists.

## Generated data

ISO 3166 reads its vendored countries/subdivisions CSV Git submodule during
build. Run `just setup` after cloning. When the submodule changes, check the
cardinalities in `crates/iso3166/tests/codegen_invariants.rs`.

Money-core generates its register from `crates/money-core/vendor/list-one.xml`.
Change vendored inputs or build scripts, never generated OUT_DIR files. Keep
NOTICE and VENDORED.md coherent with source data.

## Workflow and tests

`just` lists recipes. Run `just gate` before pushing; `just ci` also checks
package dry-runs. The root gate remains Docker-free. Docker-dependent coverage
belongs to CI and must be named as non-coverage when omitted locally.

Missing tools/targets fail rather than skip. `VERBOSE=1` exposes aggregate
output. [TOOLCHAIN-REALMS.md](docs/TOOLCHAIN-REALMS.md) defines host-first tool
resolution, exact/floor pins and setup/doctor behavior.

New recipes use `<area>-<verb>` / `*-all` naming. Aggregates compose granular
recipes; CI calls those same recipes. Every negative control must be reached by
a required check directly. A recipe only reached locally is not CI coverage.

Nextest is the ordinary runner, configured in `.config/nextest.toml`. Retries
are disabled. It omits doctests; complete ordinary-test aggregates explicitly
run `cargo test --doc`. Coverage deliberately excludes doctests. Bound container
concurrency through nextest groups.

Coverage floors live beside their rationale in cov recipes and are set only
after measurement. Thin framework adapters have behavioral and compile tests
instead of percentage floors. New behavior lands with tests.

Markdown fences need languages, tables must lint, and Taplo owns TOML formatting.

## CI

`on-pr-synced.yml` answers PRs and manual branch runs; manual dispatch compares
against the empty tree so every job runs. It has no push trigger. The ruleset
requires up-to-date branches, squash merges and linear history.
`on-main-pushed.yml` verifies that a green PR ci-success certified the landing
tree, including administrative overrides and direct pushes. CI badges read that
workflow. No job seeds another job's cache.

Path classification covers every tracked path and fails when ownership is
unknown. `DERIVED_CLASSES` in repo-policy owns fan-out with reasons and an
independent expected graph. A path-filtered job cannot depend on a narrower
condition unless it handles skipped dependencies. Tests simulate skip cascades.
Use job-level conditions, never workflow path filters for required checks.

`ci-success` is the sole required check and gathers all jobs through
re-actors/alls-green. Its needs and allowed-skip list must match the job set.
Third-party actions use full commit IDs and readable release comments. Workflow
outputs and environment variables use underscores.

Default workflow permissions are read; each workflow declares what it needs.
Workflows receiving the crates.io token target `crates-io`. That environment has
no reviewer rule: creating the GitHub Release authorizes publication.
PRs into main remain gated and reviewed; an administrative override does not
relax that rule and is not precedent.

## Commits and releases

Use lowercase imperative Conventional Commit subjects: feat, fix, refactor,
test, docs or chore, optionally scoped. Branches use `<type>/tdkc-<n>-<slug>`.
Every commit is GPG-signed and carries its lowercase Jira ticket as a standalone
paragraph. Verify signatures with `git log --show-signature`. Historical kec
prefixes remain unchanged.

Release public crates independently: update version/changelog, merge to main,
then create `<crate>-vX.Y.Z`. The release workflow verifies ancestry, version,
dependency availability and non-republication before publishing one crate.
Lockfile-only refreshes need no version bump; source or manifest changes do.
Compare manifests with crates.io before deciding which releases remain pending.

SNAP publish order: crypto, response, then crypto/response framework adapters.
Wait for the crates.io index between tiers. First publication adds
`github:pt-immer:rust-devs` as an owner; the release workflow attempts it and
add-crate-owner.yml backfills it.

## Maintenance and licensing

Update this guide when structure, tools, gates, CI ownership or release mechanics
change. Keep durable history in changelogs and current contracts in code, tests,
design documents and runbooks; do not turn this file into an incident diary.

Source is MIT OR Apache-2.0. ISO 3166 data additionally uses CC BY-SA 4.0; see
NOTICE and VENDORED.md. Money-core carries separate ISO 4217 attribution. SNAP
crates were relicensed from MIT on import.
