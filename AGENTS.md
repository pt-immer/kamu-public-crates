# Agent guide — kamu-public-crates

>
> **A Cargo workspace of 9 independently-versioned public crates** — `kamu-iso3166`, `kamu-logging`, `kamu-money-core`, and the 6-crate `kamu-snap-*` Bank Indonesia SNAP BI family. Edition 2024, MSRV 1.94, dual-licensed `MIT OR Apache-2.0`. Each crate's authoritative version lives in its `Cargo.toml` / [`CHANGELOG.md`](CHANGELOG.md); releases are per-crate (see [Commits & releases](#commits--releases)).
>
> **Plus one excluded lane under `extensions/`**, which is *not* a tenth published crate — see [Excluded lanes](#excluded-lanes).

Guidance for AI coding agents (Claude Code, GitHub Copilot, and others) working in
this repository. `CLAUDE.md` and `.github/copilot-instructions.md` are symlinks to
this file, so there is a single source of truth. For human-facing docs see
[`README.md`](README.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

## What this is

A Cargo **workspace** (GitHub repo
[`pt-immer/kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates))
publishing small, focused Rust crates — libraries and CLI apps — to crates.io:

- **`kamu-iso3166`** (`crates/iso3166`) — zero-allocation, `no_std` ISO 3166-1 /
  3166-2 country & subdivision primitives. Lookup tables are **generated at build
  time** from a vendored CSV dataset.
- **`kamu-logging`** (`crates/logging`) — structured logging over the `tracing`
  ecosystem: systemd/journald, Cloudflare-Worker `wasm32` (via `tracing-web`),
  `actix-web` request spans, and OpenTelemetry/OTLP export (`with-otlp`).
- **`kamu-money-core`** (`crates/money-core`) — exact monetary arithmetic:
  `i128` at a fixed scale of 18, compile-time currency identity, and an explicit
  residue decision before division releases its quotient. A taken residue's
  runtime backstop is suppressed during an existing unwind. Its ISO 4217 register is **generated at
  build time** from a vendored XML list, the same way `kamu-iso3166` builds its
  tables. The `postgres` / `sqlx` adapters live in the crate itself because
  `impl ToSql` for its type from elsewhere is `E0117`.
- **`kamu-snap-*`** (`crates/snap-*`) — Bank Indonesia **SNAP BI** plumbing, a
  family of 6 independently-versioned crates. `kamu-snap-crypto` (validated
  request signing/verification, HMAC/RSA-SHA256 primitives, webhook verifiers)
  and `kamu-snap-response`
  (response envelope + 61-variant error taxonomy) are framework-free leaves; the
  4 adapters `kamu-snap-{crypto,response}-{actix,axum}` bridge them to actix-web
  / axum. `kamu-snap-response` is wasm32-clean; `kamu-snap-crypto` is **not**
  (`rsa` pulls `getrandom`, which needs the consumer's `js` feature on wasm32).

Each crate **versions and releases independently** — see its own `CHANGELOG.md`.

## Excluded lanes

Some work cannot live in the main workspace without changing what the published
crates build. That work goes under `extensions/<name>/` as an **excluded nested
Cargo workspace**, and the rule is general rather than about any one lane:

- **A lane owns its own toolchain, `[patch.crates-io]`, profiles, `Cargo.lock`,
  `deny.toml` and gate.** All five are root-only or root-honoured — cargo ignores
  `[profile.*]` in a non-root member, and a `[patch]` in the main root would enter
  the nine published crates' lockfile and force a workspace-wide `allow-git` in a
  `deny.toml` whose audit is meant to cover only what ships.
- **`--workspace` at the repository root cannot see it**, because `Cargo.toml`
  `exclude`s it. That is what makes CI selectivity *structural*: a path filter
  gates whether a job runs, but it cannot change what a `--workspace` command
  builds, so exclusion is the only thing that keeps an unrelated Rust change from
  compiling PostgreSQL. Never thread `--exclude <lane>` through the aggregates
  instead — one forgotten aggregate is a silent regression.
- **Reach it through `just pg <recipe>`** — one passthrough, not a mirror of the
  lane's ~50 recipe names into a second list to keep in step.
- **`gate` covers the published crates; `gate-all` covers both.** The lane's gate
  needs Docker and takes hours, so it cannot be the barrier you run before every
  push. `gate` therefore *prints* a note when the lane has changes it did not
  cover — a stated non-coverage, which is what the no-silent-skip rule actually
  demands.
- **A lane crate is not a tenth published crate.** It carries
  `publish = false`, the README inventory does not gain a row, and
  `on-release-published.yml` recognises its `<crate>-vX.Y.Z` tag but exits before
  the crates.io step rather than failing or publishing.
- **Name a lane's recipes so nothing collides with the root's.** The two Justfiles
  are read by the same people; a name meaning one thing here and another there is
  mistyped exactly once, and the expensive direction is the one that looks cheap.
- **Policy that covers the whole tree stays at the root.** `scrub` and
  `lint-shell` are the root's and cover the lane too — `lint-shell` runs with its
  working directory set to the lane root, because scripts there resolve siblings
  relative to it. Two implementations of one policy drift until the forgotten copy
  stops matching.

The current lane is `extensions/money-pg` (the `kmoney` pgrx extension and its
YugabyteDB harness); its own `DESIGN.md` and `Justfile` carry the detail.

## Ground rules

- **Edition 2024, MSRV `1.94`.** Don't use APIs newer than 1.94; CI tests both
  `stable` and `1.94`.
- **`kamu-iso3166` needs the git submodule.** It reads its vendored ISO 3166 CSVs
  (a submodule at `crates/iso3166/vendor/iso3166-csv`) at build time. Run
  `just setup` (or `git submodule update --init --recursive`) before building.
  Dependabot bumps the pin monthly (`gitsubmodule` ecosystem in
  `.github/dependabot.yml`); the build consumes only `countries.csv` and
  `subdivisions.csv`, and when a bump touches those, re-check the pinned counts
  in `crates/iso3166/tests/codegen_invariants.rs`.
- **Never use `--all-features` across the whole workspace.** `kamu-logging`'s
  `systemd` and `wasm32` features are **mutually exclusive** (enforced by
  `compile_error!`), and `wasm32` is incompatible with both `with-actix-web` and
  `with-otlp`. A pgrx extension has the same shape for a different reason — its
  `pg15`…`pg18` features each select a different backend, so `--all-features`
  does not lint it strictly, it **fails to build it**. Select features per crate,
  as the recipes and CI do, and pin one major for anything pgrx.
- **`--workspace --exclude X` checks nothing when `X` is the only member.** It
  exits 0 and reads exactly like coverage. This is how an extension crate went
  entirely unlinted after a workspace split: the exclusion was written when the
  workspace had three members and survived a move that left it with one.
- **Lints are denials.** The workspace sets `rust.warnings = "deny"` and
  `clippy.all = "deny"`; `kamu-iso3166` additionally holds to `clippy::pedantic`.
  Keep code warning-clean.
- **Don't hand-edit generated lookup tables.** They live in `OUT_DIR`; change the
  build scripts under `crates/iso3166/build/` or the vendored CSV instead. If the
  dataset's cardinality changes, update the pinned counts in
  `crates/iso3166/tests/codegen_invariants.rs`.
- **`rust-src` is required on BOTH toolchains.** Compile-fail (`trybuild`) tests
  quote standard-library source, so without it they fail for a reason unrelated
  to the code. `just setup` installs it; never re-bless a golden produced
  without it. A test whose oracle is compiler output belongs to exactly ONE
  toolchain — keep it out of any `--workspace` sweep the MSRV job also runs.
- **`forbid(unsafe_code)`** in `kamu-iso3166` and every `kamu-snap-*` crate — no `unsafe`.
- **`kamu-snap-crypto` depends on `rsa`** (RUSTSEC-2023-0071, the "Marvin"
  timing side-channel). No patched release exists, so it is ignored in
  `deny.toml` with a rationale (SNAP BI uses RSA for signature
  generation/verification, not attacker-ciphertext decryption). Keep the ignore
  until a constant-time `rsa` ships, then drop it.
- **BRI SNAP BI signatures exclude URI queries.** The provider vector in
  `crates/snap-crypto/tests/snap_bi_recipes.rs` pins this contract. Adapters
  pass an origin-form path (`req.path()` / `uri.path()`); do not change them to
  `path_and_query` without a new provider contract and vector.

## Workflow (use `just`)

```sh
just            # list recipes
just setup      # submodules + cross targets + install missing tools
just doctor     # verify toolchain & tooling are present
just check-all  # FAST inner loop: fmt + clippy + test → compact PASS/FAIL
just gate       # published-crate local barrier; run before pushing
just ci         # Docker-free gate + metadata-derived publish dry-runs
just pg <recipe>  # run a recipe in the excluded extension lane (`just pg` lists them)
just gate-pg    # the lane's gate — hours, and needs Docker
just gate-all   # gate + gate-pg; the pre-push barrier for a lane change
```

Run `just gate` before pushing. It covers published-crate lint, Docker-free
tests, MSRV 1.94, coverage, docs, cross builds, the standalone Worker,
repository policy, and the root dependency audit as compact PASS/FAIL. CI also
runs Docker-backed database tests and package dry-runs. There is **no silent
skip**: a missing tool or target (`taplo`, `typos`, `markdownlint-cli2`, `cargo-llvm-cov`,
`cargo-nextest`, the 1.94 toolchain, the `wasm32` / `thumbv7em` targets) makes
its stage FAIL loudly — run `just setup` and `rustup toolchain install 1.94`
first. The granular recipes still exist and CI runs them directly:

```sh
just lint-all   # rustfmt + clippy + Markdown + TOML + spelling + shell + scrub
just test-all   # workspace + kamu-iso3166 / kamu-logging / kamu-snap-* feature permutations
just cov-all    # coverage gates for every gated crate
```

For the **fast inner loop while editing**, `just check-all` is the terse
fmt+clippy+test signal (compact PASS/FAIL, full output behind `VERBOSE=1`) — it
is *not* a substitute for the gate (no docs/coverage/cross-builds/MSRV). Other
terse helpers:

```sh
just check <crate>   # scoped clippy + tests for one crate (skips the workspace)
just test-fast       # cargo-nextest over the workspace + doctests
```

Cadence expectations:

- **New recipes** follow the uniform `<area>-<verb>` + `*-all` scheme
  (`lint-rust`, `build-wasm`, `cov-all`, …); aggregates compose the granular ones.
- **`cargo-nextest` is the test runner everywhere** — recipes, gate, coverage
  (`cargo llvm-cov nextest`) and CI, configured once in
  [`.config/nextest.toml`](.config/nextest.toml) rather than per invocation.
  It runs each test in its own process, which is what makes `kamu-logging`'s
  process-global subscriber testable, but it **does not run doctests**. Every
  nextest invocation is therefore paired with an explicit `cargo test --doc`;
  a new test recipe without that pair silently stops testing doc examples.
  Retries are off on purpose — a retried-into-green test is a quiet silent skip.
- **Container-backed tests are bounded in `.config/nextest.toml`**, never by a
  per-recipe flag. nextest is process-per-test and concurrent, so a suite whose
  tests each start a container will start all of them at once; a test-group cap
  binds every invocation instead of only the one someone remembered to flag.
  Docker-dependent recipes stay OUT of `just gate` — the gate must be runnable
  without a daemon, and a stage that cannot run is a stage that gets skipped.
- **Docs must pass `just lint-all`**: give every fenced code block a language,
  keep Markdown tables lint-clean, and let `taplo` own TOML formatting
  (`just fmt` / `just fmt-check`) — don't hand-align.
- **Coverage gates are enforced** (`kamu-iso3166` ≥ 98% lines, `kamu-logging`
  ≥ 70%, `kamu-money-core` ≥ 80%, `kamu-snap-crypto` ≥ 70%,
  `kamu-snap-response` ≥ 70%); floors are **measured before they are set**, and
  a floor sitting below its measurement should say why in the recipe. Land tests with
  new code. The 4 thin `kamu-snap-*-{actix,axum}` adapter crates are
  behavior-tested by the workspace suite but intentionally not
  percentage-coverage-gated.
  `kamu-logging`'s global subscriber is a process-global one-shot — test its
  error variants by constructing them in isolated unit tests, never by re-calling
  `init()`.

## Continuous integration

- **CI is path-classified per crate** (`on-pr-synced.yml`, on `pull_request`
  **and** `push: [main]`). `scripts/ci_paths.py` owns every repository surface
  and fails the `changes` job when a path has no class; `just test-repo-policy`
  also proves every tracked path remains covered. Heavy jobs use those outputs,
  so a logging-only change skips ISO jobs, a root Markdown-only change runs
  policy and docs lint, and shared build/workflow changes run everything.
  `snap` remains one umbrella because the six crates inter-depend. Shell
  ownership follows changed `*.sh` paths, not an assumed directory. Use
  job-level `if:` only — a workflow-level `paths:` filter can strand required
  checks as pending.
- **One required check: `ci-success`** (scatter → gather → and-gate). It always
  runs and uses `re-actors/alls-green` over `needs.*` — pass when every job
  succeeded or was a path-classified skip, fail on any `failure`/`cancelled` (a
  cascade-skip can't hide a failure: the failing job is itself a `need`). The
  `main` branch **ruleset requires only `ci-success`**, so jobs can be added,
  renamed, or split without touching branch protection — just keep the gate's
  `needs:` list and `allowed-skips` complete.
- **A recipe in no CI job is not a check.** `lint-shell` sat in `lint-all` and
  `gate` — both local — while CI ran only `fmt-rust-check` and `lint-docs`, and
  neither composes it. That was invisible while it covered zero tracked scripts
  and became a real hole the moment a lane arrived with 38. When adding a recipe
  to an aggregate, check whether any CI job reaches it; when a recipe's coverage
  changes, re-check.
- **A job blocked by a dependency should gate on a probe, not a flag.** The
  extension lane's container suites cannot run until `kamu-money-core` is on
  crates.io, so a job queries the registry and the suites gate on its output —
  they re-enable themselves on publication instead of waiting for someone to
  remember. The probe job always runs and emits a `::notice` naming what is
  skipped, so the gap is stated rather than silent.
- **Never give a workflow output or env var a hyphenated name.** GitHub Actions
  parses `outputs.money-pg` as `outputs.money` *minus* `pg`. The YAML still
  validates and the condition still evaluates — against something nobody
  intended. Job *IDs* may contain hyphens; outputs and env names may not.
- **CI calls `just` for build, lint, test, coverage, docs, and dependency
  audits.** Small orchestration jobs call the checked-in classifier or registry
  client directly. The Justfile remains the single source of truth for
  developer-facing checks. This includes `cargo-deny`: the root deny job installs cargo-deny via
  `taiki-e/install-action` and runs `just deny` (`cargo deny --all-features
  check`), so CI and local run the byte-identical invocation. It replaced
  `EmbarkStudios/cargo-deny-action`, whose floating `@v2` tag drifted to a
  cargo-deny release its entrypoint invokes incorrectly (`error: unrecognized
  subcommand 'warn'`) — and since `github-actions` is a Dependabot ecosystem, any
  pin would just be bumped back to the broken tag.
- **Third-party actions use full commit IDs.** Keep the readable release label
  in a trailing comment, and update the commit plus comment together.
- **Release CI** (`on-release-published.yml`) parses the `<crate>-vX.Y.Z` tag,
  verifies the manifest version and main ancestry, **refuses to re-publish a
  version already on crates.io**, checks dependency requirements against
  non-yanked sparse-index versions, and serializes per tag before publishing
  that single crate.

## Commits & releases

- **Conventional Commits**, optionally scoped: `feat:`, `fix:`, `chore:`,
  `docs:`, `refactor:`, `test:` (e.g. `feat(iso3166): add Alpha2::iter()`). Keep
  the subject imperative and lowercase.
- **Every commit carries its JIRA ticket.** Work is tracked in JIRA under the
  `kec-` prefix; name branches `<type>/kec-<n>-<slug>`. The lowercase ticket is
  its own paragraph, placed **before** the trailer block:

  ```text
  chore(deps): refresh workspace dependencies

  Bump every workspace requirement to the latest version the MSRV-1.94
  resolver allows.

  kec-1

  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
  ```

  Before, not after: git reads only the **last** paragraph as the trailer block,
  so a bare `kec-1` below `Co-Authored-By:` makes `git interpret-trailers
  --parse` return nothing and GitHub drop the co-author.
- **Every commit is GPG-signed** (`commit.gpgsign = true`); don't create unsigned
  commits — `git log --show-signature` should stay clean across the history.
- **Agent attribution**: end agent-authored commits with a `Co-Authored-By:`
  trailer naming the model that wrote them.
- **Releases are per crate**: bump the crate's `version` + `CHANGELOG.md`, merge
  to `main`, then tag a GitHub Release `<crate>-vX.Y.Z` (e.g.
  `kamu-iso3166-v0.2.0`). `on-release-published.yml` verifies the manifest
  version matches the tag and publishes that single crate. Valid crate prefixes:
  `kamu-iso3166`, `kamu-logging`, `kamu-money-core`, the 6 `kamu-snap-*`, and
  `kamu-money-pg` — which the workflow recognises and verifies against its
  manifest, then **exits before the crates.io step**. It is a `cdylib` whose
  resolvable graph goes through a root `[patch.crates-io]`, and cargo does not
  package a patch, so a published `.crate` would resolve pgrx from crates.io
  where the forked feature does not exist. The tag parser fails closed: a prefix
  it cannot attribute is an error, never a publish attempt.
- **What actually needs a release.** Only a change to a crate's own manifest or
  source warrants a version bump — a changed dependency requirement, or code. A
  workspace-wide `cargo update` lockfile refresh alone needs **no** per-crate
  bump: the refreshed `Cargo.lock` (cargo bundles it on publish) rides into each
  crate's next release. A dependency bump invisible to the public API (e.g. one
  used only behind a `pub(crate)` item) is a **patch**. To find what is pending,
  compare each crate's `Cargo.toml` `version` against its crates.io
  `max_stable_version` and tag only the crates that differ.
- **Tagging mechanics.** Tag from `main` once the version + `CHANGELOG.md` have
  landed: `gh release create <crate>-vX.Y.Z --target main`. `--target` rejects a
  short SHA (`Release.target_commitish is invalid`) — pass `main` or a full SHA.
  `on-release-published.yml` runs **once per tag**, so each crate publishes the
  moment its tag lands (not in a later batch), and crates.io refuses to
  re-publish an existing version — the tagged version must be new.
- **Snap crates publish in dependency order.** `cargo` refuses to package a crate
  whose in-workspace deps (even optional ones) are not yet on crates.io, so
  release them as: `kamu-snap-crypto` → `kamu-snap-response` →
  `kamu-snap-{crypto,response}-{actix,axum}`, waiting for the crates.io index
  between tiers (the release workflow guards this with a dep-present check).
- **First publish of a brand-new crate** makes the token's user (Ujang360) the
  sole owner. `on-release-published.yml` then adds `github:pt-immer:rust-devs`
  automatically to match the `Ujang360 + rust-devs` owner cadence (tolerant: it
  only warns if the token lacks the crates.io `change-owners` scope). Backfill
  crates published before that step — or add the team by hand — with the manual
  **Add Crate Owner** (`add-crate-owner.yml`) `workflow_dispatch`. The crates.io
  token (`SECRET_DEPLOY_CRATEIO`) is scoped to the `kamu*` glob, so new
  `kamu-snap-*` names publish without a token change.

## Keeping this guide current

`AGENTS.md` is the single source of truth (`CLAUDE.md` and
`.github/copilot-instructions.md` symlink to it). **On every task, and whenever
you notice drift, ask: "Does this change how the repo is built, tested, linted,
released, or structured — and if so, should `AGENTS.md` be updated?"** Update it
in the same change when:

- a tool, recipe, gate, or convention is added, renamed, or removed;
- you repeatedly do something this guide doesn't mention (or that contradicts it);
- a ground rule or cadence above stops matching reality.

Treat repeated divergence from this file as a bug in the file, not the work — fix
the guide so the next agent inherits what you learned.

## Licensing

Source is dual-licensed `MIT OR Apache-2.0`. `kamu-iso3166` additionally embeds ISO
3166 data under **CC BY-SA 4.0** — see `crates/iso3166/NOTICE` and
`crates/iso3166/VENDORED.md`. The `kamu-snap-*` crates were relicensed from MIT
(upstream `pt-immer/lib-snap`) to `MIT OR Apache-2.0` on import. New contributions
are accepted under `MIT OR Apache-2.0`.
