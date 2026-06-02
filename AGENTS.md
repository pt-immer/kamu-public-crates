# Agent guide — kamu-libs

Guidance for AI coding agents (Claude Code, GitHub Copilot, and others) working in
this repository. `CLAUDE.md` and `.github/copilot-instructions.md` are symlinks to
this file, so there is a single source of truth. For human-facing docs see
[`README.md`](README.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

## What this is

A Cargo **workspace** (folder `kamu-libs`, GitHub repo
[`pt-immer/kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates))
publishing small, focused Rust libraries to crates.io:

- **`kamu-iso3166`** (`crates/iso3166`) — zero-allocation, `no_std` ISO 3166-1 /
  3166-2 country & subdivision primitives. Lookup tables are **generated at build
  time** from a vendored CSV dataset.
- **`kamu-logging`** (`crates/logging`) — structured logging over the `tracing`
  ecosystem (systemd / wasm / actix).

Each crate **versions and releases independently** — see its own `CHANGELOG.md`.

## Ground rules

- **Edition 2024, MSRV `1.85`.** Don't use APIs newer than 1.85; CI tests both
  `stable` and `1.85`.
- **`kamu-iso3166` needs the git submodule.** It reads its vendored ISO 3166 CSVs
  (a submodule at `crates/iso3166/vendor/iso3166-csv`) at build time. Run
  `just setup` (or `git submodule update --init --recursive`) before building.
- **Never use `--all-features` across the whole workspace.** `kamu-logging`'s
  `systemd` and `wasm32` features are **mutually exclusive** (enforced by
  `compile_error!`) and `wasm32` is incompatible with `with-actix-web`. Select
  features per crate, as the recipes and CI do.
- **Lints are denials.** The workspace sets `rust.warnings = "deny"` and
  `clippy.all = "deny"`; `kamu-iso3166` additionally holds to `clippy::pedantic`.
  Keep code warning-clean.
- **Don't hand-edit generated lookup tables.** They live in `OUT_DIR`; change the
  build scripts under `crates/iso3166/build/` or the vendored CSV instead. If the
  dataset's cardinality changes, update the pinned counts in
  `crates/iso3166/tests/codegen_invariants.rs`.
- **`forbid(unsafe_code)`** in `kamu-iso3166` — no `unsafe`.

## Workflow (use `just`)

```sh
just            # list recipes
just setup      # submodules + cross targets + install missing tools
just doctor     # verify toolchain & tooling are present
just lint-all   # rustfmt + clippy + Markdown + TOML + spelling
just test-all   # workspace + kamu-iso3166 feature permutations
just cov-all    # coverage gates for both crates
just check-all  # lint-all + test-all + cov-all + doc + cross builds + deny
just ci         # check-all + publish dry-run (the full pipeline)
```

Run `just check-all` (or `just ci`) before proposing or committing changes — it
is the source of truth for "green". Doc-lint tools (`taplo`, `typos`,
`markdownlint-cli2`) install repo-locally under `.tools/` and `node_modules/`
via `just setup` when not already on `PATH`.

Cadence expectations:

- **New recipes** follow the uniform `<area>-<verb>` + `*-all` scheme
  (`lint-rust`, `build-wasm`, `cov-all`, …); aggregates compose the granular ones.
- **Docs must pass `just lint-all`**: give every fenced code block a language,
  keep Markdown tables lint-clean, and let `taplo` own TOML formatting
  (`just fmt` / `just fmt-check`) — don't hand-align.
- **Coverage gates are enforced** (`kamu-iso3166` ≥ 98% lines, `kamu-logging`
  ≥ 70%); land tests with new code. `kamu-logging`'s global subscriber is a
  process-global one-shot — test its error variants by constructing them in
  isolated unit tests, never by re-calling `init()`.

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
  version matches the tag and publishes that single crate.

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
`crates/iso3166/VENDORED.md`. New contributions are accepted under `MIT OR Apache-2.0`.
