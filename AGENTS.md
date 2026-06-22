# Agent guide — kamu-public-crates

>
> **A Cargo workspace of 8 independently-versioned public crates** — `kamu-iso3166`, `kamu-logging`, and the 6-crate `kamu-snap-*` Bank Indonesia SNAP BI family. Edition 2024, MSRV 1.88, dual-licensed `MIT OR Apache-2.0`. Each crate's authoritative version lives in its `Cargo.toml` / [`CHANGELOG.md`](CHANGELOG.md); releases are per-crate (see [Commits & releases](#commits--releases)).

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
- **`kamu-snap-*`** (`crates/snap-*`) — Bank Indonesia **SNAP BI** plumbing, a
  family of 6 independently-versioned crates. `kamu-snap-crypto` (HMAC/RSA
  primitives, SNAP BI recipes, webhook verifier) and `kamu-snap-response`
  (response envelope + 61-variant error taxonomy) are framework-free leaves; the
  4 adapters `kamu-snap-{crypto,response}-{actix,axum}` bridge them to actix-web
  / axum. `kamu-snap-response` is wasm32-clean; `kamu-snap-crypto` is **not**
  (`rsa` pulls `getrandom`, which needs the consumer's `js` feature on wasm32).

Each crate **versions and releases independently** — see its own `CHANGELOG.md`.

## Ground rules

- **Edition 2024, MSRV `1.88`.** Don't use APIs newer than 1.88; CI tests both
  `stable` and `1.88`.
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
  `with-otlp`. Select features per crate, as the recipes and CI do.
- **Lints are denials.** The workspace sets `rust.warnings = "deny"` and
  `clippy.all = "deny"`; `kamu-iso3166` additionally holds to `clippy::pedantic`.
  Keep code warning-clean.
- **Don't hand-edit generated lookup tables.** They live in `OUT_DIR`; change the
  build scripts under `crates/iso3166/build/` or the vendored CSV instead. If the
  dataset's cardinality changes, update the pinned counts in
  `crates/iso3166/tests/codegen_invariants.rs`.
- **`forbid(unsafe_code)`** in `kamu-iso3166` and every `kamu-snap-*` crate — no `unsafe`.
- **`kamu-snap-crypto` depends on `rsa`** (RUSTSEC-2023-0071, the "Marvin"
  timing side-channel). No patched release exists, so it is ignored in
  `deny.toml` with a rationale (SNAP BI uses RSA for signature
  generation/verification, not attacker-ciphertext decryption). Keep the ignore
  until a constant-time `rsa` ships, then drop it.

## Workflow (use `just`)

```sh
just            # list recipes
just setup      # submodules + cross targets + install missing tools
just doctor     # verify toolchain & tooling are present
just check-all  # FAST inner loop: fmt + clippy + test → compact PASS/FAIL
just gate       # THE GATE — complete CI-equivalent barrier; run before pushing
just ci         # gate + publish dry-run (the full pipeline)
```

Run `just gate` before pushing — a **green gate means CI will pass**. It runs
every check CI runs (`lint-all` + `test-all` + MSRV 1.88 + `cov-all` + `doc` +
cross builds + `deny`) as compact PASS/FAIL, and there is **no silent skip**: a
missing tool or target (`taplo`, `typos`, `markdownlint-cli2`, `cargo-llvm-cov`,
the 1.88 toolchain, the `wasm32` / `thumbv7em` targets) makes its stage FAIL
loudly — run `just setup` and `rustup toolchain install 1.88` first. The granular
recipes still exist and CI runs them directly:

```sh
just lint-all   # rustfmt + clippy + Markdown + TOML + spelling
just test-all   # workspace + kamu-iso3166 / kamu-logging / kamu-snap-* feature permutations
just cov-all    # coverage gates for every gated crate
```

For the **fast inner loop while editing**, `just check-all` is the terse
fmt+clippy+test signal (compact PASS/FAIL, full output behind `VERBOSE=1`) — it
is *not* a substitute for the gate (no docs/coverage/cross-builds/MSRV). Other
terse helpers:

```sh
just check <crate>   # scoped clippy + test for one crate (skips the workspace)
just test-fast       # cargo-nextest (failures-only) + doctests
```

Cadence expectations:

- **New recipes** follow the uniform `<area>-<verb>` + `*-all` scheme
  (`lint-rust`, `build-wasm`, `cov-all`, …); aggregates compose the granular ones.
- **Docs must pass `just lint-all`**: give every fenced code block a language,
  keep Markdown tables lint-clean, and let `taplo` own TOML formatting
  (`just fmt` / `just fmt-check`) — don't hand-align.
- **Coverage gates are enforced** (`kamu-iso3166` ≥ 98% lines, `kamu-logging`
  ≥ 70%, `kamu-snap-crypto` ≥ 70%, `kamu-snap-response` ≥ 70%); land tests with
  new code. The 4 thin `kamu-snap-*-{actix,axum}` adapter crates are
  compile-only (framework glue, no tests) and intentionally not coverage-gated.
  `kamu-logging`'s global subscriber is a process-global one-shot — test its
  error variants by constructing them in isolated unit tests, never by re-calling
  `init()`.

## Continuous integration

- **CI is path-filtered per crate** (`on-pr-synced.yml`, on `pull_request` **and**
  `push: [main]`). A `changes` job (`dorny/paths-filter`) classifies the diff into
  `iso3166` / `logging` / `snap` / `shared` / `docs`; every heavy job carries an
  `if:` so a logging-only change skips the iso3166 jobs (and vice versa), a
  root-level `*.md`-only change runs just `lint-docs` (a crate's own `*.md` change
  also runs that crate's jobs — its README is packaged on publish), and any
  shared/root/workflow change runs **everything**. The filters use **no `!`
  negation rules** — under paths-filter's default `predicate-quantifier: some` a
  negated rule matches every non-excluded file, which silently makes a filter
  `true` for unrelated changes. `snap` is one umbrella flag for all 6 snap crates (they
  inter-depend, so a base-crate change must re-test dependents); per-crate signal
  comes from the separate coverage / publish-dry-run jobs, not the filter.
  Job-level `if:` only — never a workflow-level `paths:` filter (that would strand
  required checks as pending).
- **One required check: `ci-success`** (scatter → gather → and-gate). It always
  runs and uses `re-actors/alls-green` over `needs.*` — pass when every job
  succeeded or was a path-filtered skip, fail on any `failure`/`cancelled` (a
  cascade-skip can't hide a failure: the failing job is itself a `need`). The
  `main` branch **ruleset requires only `ci-success`**, so jobs can be added,
  renamed, or split without touching branch protection — just keep the gate's
  `needs:` list and `allowed-skips` complete.
- **CI calls `just`** — every job runs `just <recipe>` (the granular recipes the
  aggregates compose), so the Justfile is the single source of truth for
  build/lint/test/coverage commands and `just <recipe>` reproduces any CI job
  locally. Job **names** are unchanged, so the `ci-success` gate is unaffected.
  Only `cargo-deny` keeps its dedicated action (advisory-DB caching).
- **Release CI** (`on-release-published.yml`) parses the `<crate>-vX.Y.Z` tag,
  verifies the manifest version, **refuses to re-publish a version already on
  crates.io**, and serializes per tag before publishing that single crate.

## Commits & releases

- **Conventional Commits**, optionally scoped: `feat:`, `fix:`, `chore:`,
  `docs:`, `refactor:`, `test:` (e.g. `feat(iso3166): add Alpha2::iter()`). Keep
  the subject imperative and lowercase.
- **Every commit is GPG-signed** (`commit.gpgsign = true`); don't create unsigned
  commits — `git log --show-signature` should stay clean across the history.
- **Agent attribution**: end agent-authored commits with a `Co-Authored-By:`
  trailer naming the model that wrote them.
- **Releases are per crate**: bump the crate's `version` + `CHANGELOG.md`, merge
  to `main`, then tag a GitHub Release `<crate>-vX.Y.Z` (e.g.
  `kamu-iso3166-v0.2.0`). `on-release-published.yml` verifies the manifest
  version matches the tag and publishes that single crate. Valid crate prefixes:
  `kamu-iso3166`, `kamu-logging`, and the 6 `kamu-snap-*`.
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
