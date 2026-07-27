set shell := ["bash", "-euo", "pipefail", "-c"]

# Prefer repo-local tool installs (`just setup`) over nothing, then fall back to
# whatever is already on PATH (system-wide installs win when no local copy exists).
export PATH := justfile_directory() + "/.tools/bin:" + justfile_directory() + "/node_modules/.bin:" + env_var("PATH")

# List available recipes
default:
    @just --list

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------

# Bootstrap a dev environment: submodules, cross targets, and any missing tools
setup:
    git submodule update --init --recursive
    rustup target add thumbv7em-none-eabi wasm32-unknown-unknown || true
    # Rust doc/ops tools -> repo-local .tools/ when not already on PATH.
    for spec in taplo:taplo-cli typos:typos-cli cargo-llvm-cov:cargo-llvm-cov cargo-deny:cargo-deny cargo-nextest:cargo-nextest; do \
      bin="${spec%%:*}"; crate="${spec##*:}"; \
      if command -v "$bin" >/dev/null 2>&1; then echo "✓ $bin already installed"; \
      else echo "installing $crate -> .tools/"; cargo install --root .tools "$crate"; fi; \
    done
    # markdownlint-cli2 (Node) -> repo-local node_modules/ when not already on PATH.
    if command -v markdownlint-cli2 >/dev/null 2>&1; then echo "✓ markdownlint-cli2 already installed"; \
    else echo "installing markdownlint-cli2 -> node_modules/"; npm install --no-fund --no-audit; fi

# Report dev-tool health; exits non-zero if a required tool is missing
doctor:
    #!/usr/bin/env bash
    set -uo pipefail
    missing=0
    check() { # name required(1|0) version-cmd...
      local name="$1" req="$2"; shift 2
      if command -v "$name" >/dev/null 2>&1; then
        printf '  ✓ %-20s %s\n' "$name" "$("$@" 2>/dev/null | head -n1)"
      else
        if [ "$req" = 1 ]; then printf '  ✗ %-20s MISSING (required)\n' "$name"; missing=1
        else printf '  · %-20s missing (optional)\n' "$name"; fi
      fi
    }
    echo "toolchain:"
    check cargo 1 cargo --version
    check rustc 1 rustc --version
    check rustup 0 rustup --version
    echo "rust components:"
    if cargo fmt --version >/dev/null 2>&1; then printf '  ✓ %-20s %s\n' rustfmt "$(cargo fmt --version 2>/dev/null)"; else printf '  ✗ %-20s MISSING (required)\n' rustfmt; missing=1; fi
    if cargo clippy --version >/dev/null 2>&1; then printf '  ✓ %-20s %s\n' clippy "$(cargo clippy --version 2>/dev/null)"; else printf '  ✗ %-20s MISSING (required)\n' clippy; missing=1; fi
    echo "doc / ops tooling:"
    check just 0 just --version
    check taplo 1 taplo --version
    check typos 1 typos --version
    check markdownlint-cli2 1 markdownlint-cli2 --version
    check cargo-llvm-cov 1 cargo-llvm-cov --version
    check cargo-deny 1 cargo-deny --version
    check cargo-nextest 1 cargo nextest --version
    check shellcheck 0 shellcheck --version
    echo "cross targets:"
    for tgt in thumbv7em-none-eabi wasm32-unknown-unknown; do
      if rustup target list --installed 2>/dev/null | grep -qx "$tgt"; then printf '  ✓ %-26s installed\n' "$tgt"; else printf '  · %-26s missing (run: just setup)\n' "$tgt"; fi
    done
    echo "vendored data:"
    if [ -f crates/iso3166/vendor/iso3166-csv/countries.csv ]; then echo "  ✓ ISO 3166 submodule initialized"; else echo "  ✗ ISO 3166 submodule NOT initialized (run: just submodules)"; missing=1; fi
    if [ "$missing" = 1 ]; then echo; echo "Some required tooling is missing — run: just setup"; exit 1; fi
    echo; echo "All required tooling present."

# ---------------------------------------------------------------------------
# Format / fix (mutating)
# ---------------------------------------------------------------------------

# Format every file type (Rust + TOML)
fmt:
    cargo fmt --all
    taplo fmt

# Check Rust formatting (the CI `rustfmt` job)
fmt-rust-check:
    cargo fmt --all --check

# Check TOML formatting
fmt-toml-check:
    taplo fmt --check

# Check formatting of every file type (Rust + TOML)
fmt-check: fmt-rust-check fmt-toml-check

# Auto-fix what tooling can (Markdown + spelling)
fix:
    markdownlint-cli2 --fix "**/*.md"
    typos -w

# ---------------------------------------------------------------------------
# Lint (read-only), per file type + aggregate
# ---------------------------------------------------------------------------

# Lint Rust workspace: deny warnings + clippy::all (default features; logging's
# systemd/wasm32 are mutually exclusive so --all-features can't span the workspace)
clippy-workspace:
    cargo clippy --workspace --all-targets -- -D warnings -D clippy::all

# Lint kamu-iso3166 with clippy::pedantic (it alone holds to pedantic)
clippy-iso3166:
    cargo clippy -p kamu-iso3166 --all-features --all-targets -- -D clippy::pedantic

# Clippy kamu-logging's non-default feature paths (with-otlp + wasm32, both
# cfg-gated off by default so clippy-workspace never sees them)
clippy-logging:
    cargo clippy -p kamu-logging --all-targets --features with-otlp -- -D warnings -D clippy::all
    cargo clippy -p kamu-logging --no-default-features --features wasm32 --target wasm32-unknown-unknown -- -D warnings -D clippy::all

# Clippy kamu-snap-* non-default feature paths (all-features + no-default crypto lib)
clippy-snap:
    cargo clippy -p kamu-snap-crypto --all-features --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-snap-crypto --no-default-features -- -D warnings -D clippy::all
    cargo clippy -p kamu-snap-response --all-features --all-targets -- -D warnings -D clippy::all

# Lint Rust: workspace clippy::all + iso3166 pedantic + ALL non-default feature perms
lint-rust: clippy-workspace clippy-iso3166 clippy-logging clippy-snap

# Lint Markdown
lint-md:
    markdownlint-cli2 "**/*.md"

# Lint TOML
lint-toml:
    taplo lint

# Spell-check sources and docs
lint-spell:
    typos

# Scan the tracked tree for PII, credentials, and the fleet's connection-string
# rules. Adopted from the incoming kamu-money repository, which held the only
# working implementation of a rule this repo states in three places and enforced
# nowhere.
#
# The host-specific needles are read from the ENVIRONMENT, never written here:
# hardcoding this machine's username in order to search for it would commit the
# very string being hunted. Every pattern below was TIGHTENED after its first
# run produced a false positive — a scanner that cries wolf is one whose next
# real finding gets waved through, so a loose pattern is worse than none.
#
# Own shebang, so a `git grep` that matches nothing (exit 1) does not kill the
# recipe under this Justfile's `set shell := [... "-euo", "pipefail" ...]`.
scrub:
    #!/usr/bin/env bash
    set -uo pipefail
    hits=0
    scan() { # label pattern [exclude-ere]
        local label="$1" pattern="$2" exclude="${3:-}"
        local found
        found=$(git grep -nIE "$pattern" -- ':!Justfile' 2>/dev/null)
        if [ -n "$exclude" ]; then
            found=$(printf '%s\n' "$found" | grep -vE "$exclude")
        fi
        if [ -n "$found" ]; then
            printf '\033[31m%s\033[0m\n%s\n\n' "$label" "$found"
            hits=$((hits+1))
        fi
    }
    # Container and CI users are not this machine's identity; naming them in a
    # Dockerfile or a captured error message is correct, not a leak.
    scan "host home paths"      "/home/[a-z][a-z0-9_-]+" "/home/(pgrx|yugabyte|postgres|runner|node|ubuntu)\b"
    # RFC 2606 reserves .invalid/.test/.example so fixtures can name an address
    # that cannot exist. Flagging those teaches the reader to skip the report.
    # `noreply@` is excluded because a no-reply address is by construction not
    # a person's contact detail — and this repo REQUIRES one, in the
    # Co-Authored-By trailer AGENTS.md documents by example.
    scan "email addresses"      "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}" \
         "noreply@|@(example|test|invalid|localhost)\.|@example\.(com|net|org)|\.(invalid|test|example)\b"
    # FOUR octets, not three: a 2-octet tail matched the decimal literal
    # "USD 10.50.50" in a parser test as though it were an RFC 1918 address.
    scan "private IPv4"         "\b(10|192\.168|172\.(1[6-9]|2[0-9]|3[01]))\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b"
    # `localhost` as an actual host token. Prose saying "127.0.0.1, never
    # localhost" is the rule being OBEYED, and flagging it trains the skim.
    scan "localhost as a host"  "(://|@|=[ ]*[\"']?|-h[ ]+)localhost([:/\"' ]|$)"
    scan "all-interface bind"   "[\"'=: ]0\.0\.0\.0[\"':]"
    scan "credential prefixes"  "(ghp_|github_pat_|sk-[a-zA-Z0-9]{20}|AKIA[0-9A-Z]{16}|BEGIN [A-Z ]*PRIVATE KEY)"
    # THE MACHINE ITSELF. A CPU model or kernel string fingerprints somebody's
    # infrastructure just as surely as a hostname, and a benchmark transcript is
    # exactly the thing that gets pasted into a design document.
    scan "cpu model names"      "\b(Xeon|EPYC|Ryzen|Core\(TM\)|Threadripper)\b"
    scan "kernel/distro string" "\b[0-9]+\.[0-9]+\.[0-9]+-[0-9]+-(arch|generic|cachyos|azure|aws|gcp)[a-z-]*\b"
    for needle in "${USER:-}" "$(hostname 2>/dev/null)"; do
        if [ -n "$needle" ] && [ "${#needle}" -ge 3 ]; then
            scan "this machine's identifiers" "\b${needle}\b"
        fi
    done
    if [ "$hits" -gt 0 ]; then
        echo "scrub: $hits category/categories need attention BEFORE any commit, tag, push or publish"
        exit 1
    fi
    echo "scrub: clean"

# Lint tracked shell scripts. A no-op until the PG lane's scripts arrive; it
# lands now so the gate's shape is settled before that import, not during it.
lint-shell:
    #!/usr/bin/env bash
    set -uo pipefail
    files=$(git ls-files '*.sh')
    if [ -z "$files" ]; then echo "lint-shell: no shell scripts tracked"; exit 0; fi
    # shellcheck disable=SC2086
    shellcheck $files

# Lint docs the way the CI `lint docs` job does: TOML fmt-check + Markdown +
# TOML lint + spelling + the PII/secret scan (no Rust). lint-all adds Rust.
# `scrub` is here as well as in lint-all on purpose: this is the job a
# root-level `*.md`-only PR runs alone, and prose is where a leaked hostname
# arrives far more often than Rust is.
lint-docs: fmt-toml-check lint-md lint-toml lint-spell scrub

# Lint every file type (formatting + Rust + Markdown + TOML + spelling + shell
# + the PII/secret scan)
lint-all: fmt-check lint-rust lint-md lint-toml lint-spell lint-shell scrub

# ---------------------------------------------------------------------------
# Tests / docs / supply chain
# ---------------------------------------------------------------------------

# Test kamu-iso3166 feature permutations (the CI `test (kamu-iso3166 feature
# permutations)` job): all-features + serde-only, plus all-features doctests.
test-iso3166:
    cargo nextest run -p kamu-iso3166 --all-features
    cargo nextest run -p kamu-iso3166 --no-default-features --features serde
    # nextest cannot run doctests. This finds none today (kamu-logging owns the
    # only doctests in the workspace) but still compiles them, so a broken `///`
    # example fails here the moment one is written.
    cargo test -p kamu-iso3166 --all-features --doc

# Test the workspace plus kamu-iso3166 / kamu-snap-* feature permutations.
# Every test binary runs under cargo-nextest (process-per-test isolation, see
# .config/nextest.toml); doctests are a SEPARATE `cargo test --doc` pass because
# nextest does not run them.
test-all:
    cargo nextest run --workspace
    # kamu-logging OTLP path (a default run exercises systemd+actix, not OTLP):
    # covers BatchSpanProcessor + the drain helpers + the runtime test.
    cargo nextest run -p kamu-logging --features with-otlp
    cargo nextest run -p kamu-iso3166 --all-features
    cargo nextest run -p kamu-iso3166 --no-default-features --features serde
    cargo nextest run -p kamu-snap-crypto --all-features
    cargo nextest run -p kamu-snap-response --all-features
    # Doctests, which nextest skips by design. kamu-logging holds the only ones
    # in the workspace, and the default-feature run reaches them.
    cargo test --workspace --doc
    # Leaf lib must also compile with default features off (HMAC/RSA-only crypto,
    # no snap-bi/webhook). Tests pull snap-bi, so this is a lib check, not a test.
    cargo check -p kamu-snap-crypto --no-default-features

# Build workspace docs the way docs.rs does (CI `docs (workspace)` job)
doc-workspace:
    RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps

# Build kamu-iso3166 docs with all features (CI `docs (kamu-iso3166)` job)
doc-iso3166:
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-iso3166 --no-deps --all-features

# Build kamu-snap-response docs with all features — gains the `crypto` feature
# (docs.rs parity). Other snap crates' all-features == default, already covered.
doc-snap-response:
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-snap-response --no-deps --all-features

# Build docs the way docs.rs does
doc: doc-workspace doc-iso3166 doc-snap-response

# Supply-chain audit. `--all-features` is a GLOBAL flag (before the subcommand)
# — it widens the audited dependency graph to every optional dep, matching what
# CI audits. This recipe IS the CI deny job (no dedicated action), so the local
# and CI cargo-deny invocations are identical — no version/flag drift.
deny:
    cargo deny --all-features check

# ---------------------------------------------------------------------------
# Coverage
# ---------------------------------------------------------------------------

# Every gate below measures through `cargo llvm-cov nextest`, so coverage runs
# the same binaries the same way `just test-all` does — one runner, one result.
# Verified identical to the old `cargo llvm-cov` (cargo-test runner) numbers on
# kamu-iso3166: 99.67% lines / 96.83% regions either way. Doctests are excluded
# from coverage under both runners (cargo-llvm-cov needs nightly for those), so
# the floors mean the same thing they always did.

# Coverage gate for kamu-iso3166 (also emits target/lcov.info for the CI artifact)
cov:
    cargo llvm-cov nextest -p kamu-iso3166 --all-features --ignore-filename-regex 'generated|build/' --fail-under-lines 98 --lcov --output-path target/lcov.info

# Coverage gate for kamu-logging (no --all-features: systemd XOR wasm32)
cov-logging:
    cargo llvm-cov nextest -p kamu-logging --fail-under-lines 70

# Coverage gate for kamu-snap-crypto. Floor 70 (measured ~74%): the default-on
# `webhook` providers ship without tests upstream; raising this is future work.
cov-snap-crypto:
    cargo llvm-cov nextest -p kamu-snap-crypto --all-features --fail-under-lines 70

# Coverage gate for kamu-snap-response. Floor 70 (measured ~74%); `category.rs`
# is currently untested upstream. The 4 thin actix/axum adapter crates have no
# tests (framework-bound glue) and are intentionally compile-only, not gated.
cov-snap-response:
    cargo llvm-cov nextest -p kamu-snap-response --all-features --fail-under-lines 70

# Coverage gates for every gated crate
cov-all: cov cov-logging cov-snap-crypto cov-snap-response

# HTML coverage report for the whole workspace; prints the output path
cov-html:
    cargo llvm-cov nextest --workspace --ignore-filename-regex 'generated|build/' --html
    @echo "report: target/llvm-cov/html/index.html"

# ---------------------------------------------------------------------------
# Build variants
# ---------------------------------------------------------------------------

# no_std cross-compile of kamu-iso3166 (needs: rustup target add thumbv7em-none-eabi)
build-nostd:
    cargo build -p kamu-iso3166 --no-default-features --target thumbv7em-none-eabi

# Compile kamu-logging for wasm32 (guards the wasm path host tests can't run)
build-wasm:
    cargo build -p kamu-logging --no-default-features --features wasm32 --target wasm32-unknown-unknown

# Compile kamu-snap-response for wasm32 (it is wasm-clean). kamu-snap-crypto is
# NOT wasm-clean: rsa -> getrandom needs the consumer's `js` feature, so it is
# deliberately not built here.
build-wasm-snap:
    cargo check -p kamu-snap-response --no-default-features --target wasm32-unknown-unknown

# Type-check the Cloudflare Worker example (a cdylib excluded from the workspace)
check-worker-example:
    cargo check --manifest-path crates/logging/examples/cloudflare-worker/Cargo.toml --target wasm32-unknown-unknown

# ---------------------------------------------------------------------------
# Publish / vendored data / housekeeping
# ---------------------------------------------------------------------------

# Dry-run publish a single crate, e.g. `just publish-dry kamu-iso3166`
publish-dry crate:
    cargo publish -p {{ crate }} --dry-run

# Dry-run publish the crates that can be packaged standalone: iso3166, logging,
# and kamu-snap-crypto are leaves (every dep is already on crates.io).
# kamu-snap-response and the 4 snap adapter crates CANNOT be dry-run until their
# in-workspace base crate is published — cargo's package step requires every
# declared dependency (even an OPTIONAL one) to resolve on crates.io, and
# --no-verify does NOT skip that check ("no matching package named
# kamu-snap-crypto found"). They are covered by `just check-all`
# (workspace build/clippy/test/doc) and published in dependency order
# (crypto -> response -> adapters) via on-release-published.yml.
publish-all:
    cargo publish -p kamu-iso3166 --dry-run
    cargo publish -p kamu-logging --dry-run
    cargo publish -p kamu-snap-crypto --dry-run

# Initialize the vendored ISO 3166 data submodule
submodules:
    git submodule update --init --recursive

# Remove build artifacts
clean:
    cargo clean

# ---------------------------------------------------------------------------
# Aggregates
# ---------------------------------------------------------------------------

# THE GATE — the complete, CI-equivalent barrier: a green gate means CI passes.
# Runs every check CI runs (lint-all + test-all + MSRV 1.94 + cov-all + doc +
# cross builds + deny) as compact PASS/FAIL lines; full output for failed stages,
# or everything with `VERBOSE=1 just gate`. There is NO silent skip: a missing
# tool or target (taplo, typos, markdownlint, cargo-llvm-cov, the 1.94 toolchain,
# the wasm32 / thumbv7em targets) makes its stage FAIL loudly — run `just setup`
# (and `rustup toolchain install 1.94`) first. `just check-all` is the fast loop.
# Complete CI-equivalent barrier — a green gate means CI passes; run before push.
gate:
    #!/usr/bin/env bash
    set -uo pipefail
    names=("lint-all" "test-all" "msrv(1.94)" "cov-all" "doc" "build-nostd" "build-wasm" "build-wasm-snap" "deny")
    cmds=("just lint-all"
          "just test-all"
          "cargo +1.94 nextest run --workspace && cargo +1.94 test --workspace --doc --quiet"
          "just cov-all"
          "just doc"
          "just build-nostd"
          "just build-wasm"
          "just build-wasm-snap"
          "just deny")
    declare -a rcs outs
    fail=0
    for i in "${!names[@]}"; do
      outs[$i]=$(eval "${cmds[$i]}" 2>&1); rcs[$i]=$?
      if [ "${rcs[$i]}" -eq 0 ]; then printf '  PASS  %s\n' "${names[$i]}"; else printf '  FAIL  %s\n' "${names[$i]}"; fail=1; fi
    done
    if [ "${VERBOSE:-0}" = "1" ]; then
      for i in "${!names[@]}"; do printf '\n=== %s ===\n%s\n' "${names[$i]}" "${outs[$i]}"; done
    elif [ "$fail" -ne 0 ]; then
      for i in "${!names[@]}"; do [ "${rcs[$i]}" -ne 0 ] && printf '\n=== %s (FAILED) ===\n%s\n' "${names[$i]}" "${outs[$i]}"; done
    fi
    exit "$fail"

# The full pipeline: everything the gate runs, plus a publish dry-run.
ci: gate publish-all

# ---------------------------------------------------------------------------
# Agentic (token-thrifty): compact signal for AI agents; firehose behind VERBOSE=1
#
# These recipes are for AI coding agents (and humans who want a terse loop).
# They print the minimum signal needed to pick the next action and keep the full
# output behind VERBOSE=1, so a green run costs a handful of lines instead of a
# few thousand. CI does NOT use them — CI runs the explicit lint-all / test-all /
# cov-all recipes above.
#
# Test verbosity is NOT one of the things these recipes vary: every nextest run
# in this file, agentic or not, reports through .config/nextest.toml. A green
# run is a summary line everywhere, and CI's completeness comes from the run
# counts ("Starting N tests across M binaries") plus the `ci` profile's
# immediate-final failure output — not from listing every passing test.
# ---------------------------------------------------------------------------

# Fast inner-loop check with a compact PASS/FAIL summary: fmt + clippy + test on
# the active toolchain. On failure prints ONLY the failing step's output;
# `VERBOSE=1 just check-all` prints everything. This is NOT the pre-push barrier
# (no docs/coverage/cross-builds/MSRV).
# Fast inner loop: fmt + clippy + test — run `just gate` before pushing.
check-all:
    #!/usr/bin/env bash
    set -uo pipefail
    names=("fmt" "clippy" "test")
    cmds=("cargo fmt --all --check"
          "cargo clippy --workspace --all-targets --message-format=short -- -D warnings -D clippy::all"
          "cargo nextest run --workspace && cargo test --workspace --doc --quiet")
    declare -a rcs outs
    fail=0
    for i in "${!names[@]}"; do
      outs[$i]=$(eval "${cmds[$i]}" 2>&1); rcs[$i]=$?
      if [ "${rcs[$i]}" -eq 0 ]; then printf '  PASS  %s\n' "${names[$i]}"; else printf '  FAIL  %s\n' "${names[$i]}"; fail=1; fi
    done
    if [ "${VERBOSE:-0}" = "1" ]; then
      for i in "${!names[@]}"; do printf '\n=== %s ===\n%s\n' "${names[$i]}" "${outs[$i]}"; done
    elif [ "$fail" -ne 0 ]; then
      for i in "${!names[@]}"; do [ "${rcs[$i]}" -ne 0 ] && printf '\n=== %s (FAILED) ===\n%s\n' "${names[$i]}" "${outs[$i]}"; done
    fi
    exit "$fail"

# Skips the rest of the workspace so an agent only reads relevant output.
# Scoped check for ONE crate (clippy short + tests), e.g. `just check kamu-iso3166`.
check crate:
    #!/usr/bin/env bash
    set -uo pipefail
    fail=0
    cargo clippy -p '{{ crate }}' --all-targets --message-format=short -- -D warnings -D clippy::all || fail=1
    cargo nextest run -p '{{ crate }}' || fail=1
    cargo test -p '{{ crate }}' --doc --quiet || fail=1
    exit "$fail"

# Terse test run: cargo-nextest over the workspace + the doctests it cannot run.
# Verbosity comes from .config/nextest.toml (status-level = fail), not a flag, so
# every nextest invocation in this file reports the same way. cargo-nextest is
# REQUIRED — there is deliberately no `cargo test` fallback, because silently
# swapping the runner would change test isolation without saying so; a missing
# binary should fail loudly and send you to `just setup`.
test-fast:
    #!/usr/bin/env bash
    set -uo pipefail
    cargo nextest run --workspace
    cargo test --workspace --doc --quiet
