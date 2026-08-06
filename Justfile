set shell := ["bash", "-euo", "pipefail", "-c"]

# Prefer repo-local tool installs (`just setup`) over nothing, then fall back to
# whatever is already on PATH (system-wide installs win when no local copy exists).
export PATH := justfile_directory() + "/.tools/bin:" + justfile_directory() + "/node_modules/.bin:" + env_var("PATH")

# List available recipes
[doc("List available recipes.")]
default:
    @just --list

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------

# Install the exact root-gate environment from .config/dev-tools.json.
[doc("Install pinned toolchains, targets, and development tools.")]
setup:
    python3 scripts/dev_environment.py setup

# Report every prerequisite reached by the root gate.
[doc("Verify every tool and Rust component required by the gates.")]
doctor:
    python3 scripts/dev_environment.py doctor

# ---------------------------------------------------------------------------
# Format / fix (mutating)
# ---------------------------------------------------------------------------

# Format every file type (Rust + TOML)
[doc("Format Rust and TOML files.")]
fmt:
    cargo fmt --all
    taplo fmt

# Check Rust formatting (the CI `rustfmt` job)
[doc("Verify Rust formatting.")]
fmt-rust-check:
    cargo fmt --all --check

# Check TOML formatting
[doc("Verify TOML formatting.")]
fmt-toml-check:
    taplo fmt --check

# Check formatting of every file type (Rust + TOML)
[doc("Verify Rust and TOML formatting.")]
fmt-check: fmt-rust-check fmt-toml-check

# Auto-fix what tooling can (Markdown + spelling)
[doc("Apply Markdown and spelling fixes.")]
fix:
    markdownlint-cli2 --fix "**/*.md"
    typos -w

# ---------------------------------------------------------------------------
# Lint (read-only), per file type + aggregate
# ---------------------------------------------------------------------------

# Lint Rust workspace: deny warnings + clippy::all (default features; logging's
# systemd/wasm32 are mutually exclusive so --all-features can't span the workspace)
[doc("Clippy the root workspace's default feature set.")]
clippy-workspace:
    cargo clippy --workspace --all-targets -- -D warnings -D clippy::all

# Lint kamu-iso3166 with clippy::pedantic (it alone holds to pedantic)
[doc("Clippy kamu-iso3166 with all features and pedantic lints.")]
clippy-iso3166:
    cargo clippy -p kamu-iso3166 --all-features --all-targets -- -D clippy::pedantic

# `correlation` exists so a library can take the traceparent parser without
# taking a subscriber. Assert that literally: nothing that installs or sinks
# logs may appear in its graph. `-e normal` keeps the unconditional actix-web
# dev-dependency out of the answer.
[doc("Assert kamu-logging's correlation-only graph pulls no subscriber or sink.")]
deps-logging-correlation:
    #!/usr/bin/env bash
    set -euo pipefail
    graph=$(cargo tree -p kamu-logging --no-default-features --features correlation -e normal --prefix none --format '{p}')
    banned=$(printf '%s\n' "$graph" | grep -E '^(console|opentelemetry|tracing-journald|tracing-subscriber|tracing-web)[ -]' || true)
    if [ -n "$banned" ]; then
        echo "deps-logging-correlation: correlation must not pull a subscriber or sink:" >&2
        printf '%s\n' "$banned" >&2
        exit 1
    fi
    echo "deps-logging-correlation: clean"

# Clippy kamu-logging's non-default feature paths (with-otlp, wasm32, and the
# two subscriber-free sets, all cfg-gated off by default so clippy-workspace
# never sees them)
[doc("Clippy every supported kamu-logging feature set.")]
clippy-logging: deps-logging-correlation
    cargo clippy -p kamu-logging --all-targets --features with-otlp -- -D warnings -D clippy::all
    cargo clippy -p kamu-logging --no-default-features --features wasm32 --target wasm32-unknown-unknown -- -D warnings -D clippy::all
    cargo clippy -p kamu-logging --no-default-features --features correlation --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-logging --no-default-features --features with-actix-web -- -D warnings -D clippy::all

# Clippy kamu-snap-* non-default feature paths (all-features + no-default crypto lib)
[doc("Clippy every supported SNAP feature set.")]
clippy-snap:
    cargo clippy -p kamu-snap-crypto --all-features --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-snap-crypto --no-default-features -- -D warnings -D clippy::all
    cargo clippy -p kamu-snap-response --all-features --all-targets -- -D warnings -D clippy::all

# `kamu-money-core` defines stricter crate-local lints; exercise both feature
# surfaces here.
[doc("Clippy kamu-money-core with and without optional features.")]
clippy-money:
    cargo clippy -p kamu-money-core --all-features --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-money-core --no-default-features -- -D warnings -D clippy::all

# Lint every supported Rust feature surface.
[doc("Run every root-workspace Clippy check.")]
lint-rust: clippy-workspace clippy-iso3166 clippy-logging clippy-money clippy-snap

# Lint Markdown
[doc("Lint Markdown files.")]
lint-md:
    markdownlint-cli2 "**/*.md"

# Lint TOML
[doc("Lint TOML files.")]
lint-toml:
    taplo lint

# Spell-check sources and docs
[doc("Spell-check source and documentation.")]
lint-spell:
    typos

# Scan tracked content for PII, credentials, network identities, and host
# fingerprints. Host-specific needles come from the environment and are never
# embedded here. Expected fixture identities are excluded narrowly.
[doc("Scan tracked files for credentials, PII, and host identity.")]
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
    # Generic container and CI accounts are not host identities.
    scan "host home paths"      "/home/[a-z][a-z0-9_-]+" "/home/(pgrx|yugabyte|postgres|runner|node|ubuntu)\b"
    # Reserved fixture domains and `noreply@` are not personal contacts.
    scan "email addresses"      "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}" \
         "noreply@|@(example|test|invalid|localhost)\.|@example\.(com|net|org)|\.(invalid|test|example)\b"
    # Require four octets so decimal fixtures cannot resemble private IPs.
    scan "private IPv4"         "\b(10|192\.168|172\.(1[6-9]|2[0-9]|3[01]))\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b"
    # Match `localhost` only where it is used as a host token.
    scan "localhost as a host"  "(://|@|=[ ]*[\"']?|-h[ ]+)localhost([:/\"' ]|$)"
    scan "all-interface bind"   "[\"'=: ]0\.0\.0\.0[\"':]"
    scan "credential prefixes"  "(ghp_|github_pat_|sk-[a-zA-Z0-9]{20}|AKIA[0-9A-Z]{16}|BEGIN [A-Z ]*PRIVATE KEY)"
    # CPU and kernel identifiers can fingerprint benchmark hosts.
    scan "cpu model names"      "\b(Xeon|EPYC|Ryzen|Core\(TM\)|Threadripper)\b"
    scan "kernel/distro string" "\b[0-9]+\.[0-9]+\.[0-9]+-[0-9]+-(arch|generic|cachyos|azure|aws|gcp)[a-z-]*\b"
    # Read this host's names from the environment; ignore generic service users.
    for needle in "${USER:-}" "$(hostname 2>/dev/null)"; do
        case "$needle" in
            runner|ubuntu|node|postgres|pgrx|yugabyte|root|admin|user|build|ci) continue ;;
        esac
        if [ -n "$needle" ] && [ "${#needle}" -ge 3 ]; then
            scan "this machine's identifiers" "\b${needle}\b"
        fi
    done
    if [ "$hits" -gt 0 ]; then
        echo "scrub: $hits category/categories need attention BEFORE any commit, tag, push or publish"
        exit 1
    fi
    echo "scrub: clean"

# ShellCheck follows sourced files and runs lane scripts from their own root so
# relative imports resolve.
[doc("ShellCheck every tracked shell script.")]
lint-shell:
    #!/usr/bin/env bash
    set -uo pipefail
    if ! command -v shellcheck >/dev/null 2>&1; then
        echo "lint-shell: ShellCheck is missing; run 'just setup'" >&2
        exit 1
    fi
    lane=extensions/money-pg
    rc=0
    shell_files=$(git ls-files --cached --others --exclude-standard '*.sh')
    host=$(printf '%s\n' "$shell_files" | grep -v "^$lane/" || true)
    lane_files=$(printf '%s\n' "$shell_files" | grep "^$lane/" | sed "s|^$lane/||" || true)
    if [ -n "$host" ]; then
        # shellcheck disable=SC2086
        shellcheck -x $host || rc=1
    fi
    if [ -n "$lane_files" ]; then
        # shellcheck disable=SC2086
        (cd "$lane" && shellcheck -x $lane_files) || rc=1
    fi
    n_host=$(printf '%s' "$host" | grep -c . || true)
    n_lane=$(printf '%s' "$lane_files" | grep -c . || true)
    if [ "$rc" -eq 0 ]; then
        echo "lint-shell: clean over $n_host repository + $n_lane lane script(s)"
    fi
    exit "$rc"

# Complete docs-only CI surface, including the scrub for Markdown-only changes.
[doc("Run formatting, Markdown, TOML, spelling, and scrub checks for docs.")]
lint-docs: fmt-toml-check lint-md lint-toml lint-spell scrub

# Lint every file type (formatting + Rust + Markdown + TOML + spelling + shell
# + the PII/secret scan)
[doc("Run every root-workspace lint and repository policy check.")]
lint-all: fmt-check lint-rust lint-md lint-toml lint-spell lint-shell scrub

# ---------------------------------------------------------------------------
# Tests / docs / supply chain
# ---------------------------------------------------------------------------

# Test `kamu-iso3166` feature permutations and doctests.
[doc("Test kamu-iso3166 feature permutations and doctests.")]
test-iso3166:
    cargo nextest run -p kamu-iso3166 --all-features
    cargo nextest run -p kamu-iso3166 --no-default-features --features serde
    # Nextest does not run doctests.
    cargo test -p kamu-iso3166 --all-features --doc

# Test `kamu-money-core` feature surfaces that do not require Docker.
[doc("Test kamu-money-core's Docker-free feature sets and doctests.")]
test-money:
    cargo nextest run -p kamu-money-core
    cargo nextest run -p kamu-money-core --features serde
    cargo nextest run -p kamu-money-core --features postgres -E 'not binary(pg_roundtrip)'
    # Nextest does not run doctests.
    cargo test -p kamu-money-core --all-features --doc --quiet

# Docker-backed adapter tests. Native-extension tests belong to the PG lane.
# Concurrency is bounded by `.config/nextest.toml`.
[doc("Run kamu-money-core's Docker-backed database tests.")]
test-money-db:
    cargo nextest run -p kamu-money-core --all-features -E 'binary(pg_roundtrip) or binary(sqlx_roundtrip)'

# Test every Docker-free feature surface; run doctests separately from nextest.
[doc("Run every Docker-free root-workspace test matrix.")]
test-all:
    # Trybuild goldens belong to `just test-money` on the pinned compiler.
    cargo nextest run --workspace -E 'not binary(compile_fail)'
    # The default logging run does not reach OTLP.
    cargo nextest run -p kamu-logging --features with-otlp
    # Correlation must work with no subscriber installed at all.
    cargo nextest run -p kamu-logging --no-default-features --features correlation
    cargo nextest run -p kamu-iso3166 --all-features
    cargo nextest run -p kamu-iso3166 --no-default-features --features serde
    cargo nextest run -p kamu-snap-crypto --all-features
    cargo nextest run -p kamu-snap-response --all-features
    # Docker-backed adapters and native-extension cases have separate owners.
    cargo nextest run -p kamu-money-core --all-features -E 'not (binary(compile_fail) or binary(pg_roundtrip) or binary(sqlx_roundtrip) or binary(pg_native_column) or binary(yugabyte_roundtrip))'
    # Nextest does not run doctests.
    cargo test --workspace --doc
    # Prove the HMAC/RSA-only leaf builds without SNAP BI or webhook features.
    cargo check -p kamu-snap-crypto --no-default-features

# Build workspace docs the way docs.rs does (CI `docs (workspace)` job)
[doc("Build root-workspace documentation with docs.rs settings.")]
doc-workspace:
    RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps

# Build kamu-iso3166 docs with all features (CI `docs (kamu-iso3166)` job)
[doc("Build kamu-iso3166 documentation with all features.")]
doc-iso3166:
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-iso3166 --no-deps --all-features

# Build kamu-snap-response docs with all features — gains the `crypto` feature
# (docs.rs parity). Other snap crates' all-features == default, already covered.
[doc("Build kamu-snap-response documentation with all features.")]
doc-snap-response:
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-snap-response --no-deps --all-features

# Build all feature-gated `kamu-money-core` modules as docs.rs does.
[doc("Build kamu-money-core documentation with all features.")]
doc-money:
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-money-core --no-deps --all-features

# Build docs the way docs.rs does
[doc("Build every documentation target.")]
doc: doc-workspace doc-iso3166 doc-money doc-snap-response

# Audit the full optional dependency graph with the same invocation as CI.
[doc("Audit dependency advisories, licenses, bans, and sources.")]
deny:
    cargo deny --all-features check

# ---------------------------------------------------------------------------
# Coverage
# ---------------------------------------------------------------------------

# Coverage uses the same nextest runner as ordinary tests. Doctests are outside
# these measurements.

# Coverage gate for kamu-iso3166 (also emits target/lcov.info for the CI artifact)
[doc("Enforce kamu-iso3166's line-coverage floor.")]
cov:
    cargo llvm-cov nextest -p kamu-iso3166 --all-features --ignore-filename-regex 'generated|build/' --fail-under-lines 98 --lcov --output-path target/lcov.info

# Coverage gate for kamu-logging (no --all-features: systemd XOR wasm32)
[doc("Enforce kamu-logging's line-coverage floor.")]
cov-logging:
    # Measured 92.67% after the 2.0 ownership/Actix tests. Keep 4.67 points for
    # target-only terminal, environment, and wasm branches the host run cannot hit.
    cargo llvm-cov nextest -p kamu-logging --fail-under-lines 88

# Coverage gate for kamu-snap-crypto. Floor 70 (measured ~74%): the default-on
# `webhook` providers ship without tests upstream; raising this is future work.
[doc("Enforce kamu-snap-crypto's line-coverage floor.")]
cov-snap-crypto:
    cargo llvm-cov nextest -p kamu-snap-crypto --all-features --fail-under-lines 70

# Coverage gate for kamu-snap-response. Floor 85, measured 90.11% lines
# (687 regions, 465 lines). The five-point margin absorbs LLVM
# instrumentation drift. Framework adapters are behavior-tested by the workspace
# suite but intentionally not percentage-gated.
[doc("Enforce kamu-snap-response's line-coverage floor.")]
cov-snap-response:
    cargo llvm-cov nextest -p kamu-snap-response --all-features --fail-under-lines 85

# The 80% floor is below the measured 84.89% because Docker-backed driver paths
# are excluded. `build/` is covered by register tests; trybuild runs no library
# code.
[doc("Enforce kamu-money-core's Docker-free line-coverage floor.")]
cov-money:
    cargo llvm-cov nextest -p kamu-money-core --all-features -E 'not (binary(compile_fail) or binary(pg_roundtrip) or binary(sqlx_roundtrip) or binary(pg_native_column) or binary(yugabyte_roundtrip))' --ignore-filename-regex 'build/' --fail-under-lines 80

# Coverage gates for every gated crate
[doc("Enforce every configured line-coverage floor.")]
cov-all: cov cov-logging cov-money cov-snap-crypto cov-snap-response

# HTML coverage report for the whole workspace; prints the output path
[doc("Generate an HTML coverage report for the root workspace.")]
cov-html:
    cargo llvm-cov nextest --workspace --ignore-filename-regex 'generated|build/' --html
    @echo "report: target/llvm-cov/html/index.html"

# ---------------------------------------------------------------------------
# Build variants
# ---------------------------------------------------------------------------

# no_std cross-compile of kamu-iso3166 (needs: rustup target add thumbv7em-none-eabi)
[doc("Cross-compile kamu-iso3166 for thumbv7em-none-eabi.")]
build-nostd:
    cargo build -p kamu-iso3166 --no-default-features --target thumbv7em-none-eabi

# Compile kamu-logging for wasm32 (guards the wasm path host tests can't run)
[doc("Cross-compile kamu-logging for wasm32.")]
build-wasm:
    cargo build -p kamu-logging --no-default-features --features wasm32 --target wasm32-unknown-unknown

# Compile kamu-snap-response for wasm32 (it is wasm-clean). kamu-snap-crypto is
# NOT wasm-clean: rsa -> getrandom needs the consumer's `js` feature, so it is
# deliberately not built here.
[doc("Cross-compile the wasm-clean SNAP response crate.")]
build-wasm-snap:
    cargo check -p kamu-snap-response --no-default-features --target wasm32-unknown-unknown

# Build the standalone Cloudflare Worker with both lockfiles.
[doc("Check the standalone Cloudflare Worker example.")]
check-worker-example:
    cargo check --manifest-path crates/logging/examples/cloudflare-worker/Cargo.toml --target wasm32-unknown-unknown
    npm --prefix crates/logging/examples/cloudflare-worker ci --no-fund --no-audit
    npm --prefix crates/logging/examples/cloudflare-worker run build

# Examples ship inside the published packages, so a broken one is a broken release
# artifact. Each crate is built under the features its examples declare.
[doc("Type-check every crate's examples.")]
check-examples:
    cargo check --examples -p kamu-logging --features systemd,with-actix-web
    cargo check --examples -p kamu-money-core --features serde
    cargo check --examples -p kamu-snap-crypto --features snap-bi
    cargo check --examples -p kamu-snap-response

# Test fail-closed CI classification, registry probing, and standalone-package
# ownership; then prove every tracked path is classified.
[doc("Test CI path ownership and workflow policy.")]
test-repo-policy:
    python3 -m unittest discover -s scripts -p 'test_*.py'
    python3 scripts/ci_paths.py check-tracked

# ---------------------------------------------------------------------------
# Publish / vendored data / housekeeping
# ---------------------------------------------------------------------------

# Dry-run publish a single crate, e.g. `just publish-dry kamu-iso3166`
[doc("Dry-run packaging and publishing for one crate.")]
publish-dry crate:
    cargo publish -p {{ crate }} --dry-run --allow-dirty

# Package workspace members together so unpublished workspace dependencies are
# available through Cargo's temporary registry during verification.
[doc("Dry-run every publishable root-workspace crate.")]
publish-all:
    cargo publish --workspace --dry-run --allow-dirty

# Initialize the vendored ISO 3166 data submodule
[doc("Initialize all Git submodules recursively.")]
submodules:
    git submodule update --init --recursive

# Remove build artifacts
[doc("Remove root-workspace build artifacts.")]
clean:
    cargo clean

# ---------------------------------------------------------------------------
# Aggregates
# ---------------------------------------------------------------------------

# Complete public-workspace barrier. Missing tools and targets fail; use
# `VERBOSE=1` for every stage's output.
[doc("Run the complete Docker-free gate for the nine public crates.")]
gate:
    #!/usr/bin/env bash
    set -uo pipefail
    names=("lint-all" "test-all" "test-money" "test-repo-policy" "msrv(1.94.0)" "cov-all" "doc" "build-nostd" "build-wasm" "build-wasm-snap" "check-worker-example" "check-examples" "deny")
    cmds=("just lint-all"
          "just test-all"
          "just test-money"
          "just test-repo-policy"
          "cargo +1.94.0 nextest run --workspace -E 'not binary(compile_fail)' && cargo +1.94.0 test --workspace --doc --quiet"
          "just cov-all"
          "just doc"
          "just build-nostd"
          "just build-wasm"
          "just build-wasm-snap"
          "just check-worker-example"
          "just check-examples"
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
    # State explicitly when this Docker-free gate did not cover the excluded lane.
    if [ -n "$(git status --porcelain --untracked-files=all -- extensions/money-pg)" ]; then
      echo
      echo "  NOTE  extensions/money-pg has changes this gate did NOT cover."
      echo "        Run 'just gate-all' before pushing them."
    fi
    exit "$fail"

# Passthrough keeps the lane's recipe inventory in its own Justfile.
[doc("Run a recipe in the excluded PostgreSQL extension lane.")]
pg *ARGS:
    cd extensions/money-pg && just {{ ARGS }}

# The PostgreSQL lane's developer gate. Hours and Docker-backed, but excludes the native YB
# release proof (`just pg gate-pg-release`).
[doc("Run the Docker-backed developer gate for the extension lane.")]
gate-pg:
    cd extensions/money-pg && just gate-pg

# Pre-push barrier for a change under extensions/money-pg: the nine public crates plus the lane's
# developer gate. Extension releases also require `just pg gate-pg-release`.
[doc("Run the public-crate gate and extension developer gate.")]
gate-all: gate gate-pg

# Local published-crate gate plus the workspace publish dry-run.
[doc("Run the public-crate gate plus every package dry-run.")]
ci: gate publish-all

# ---------------------------------------------------------------------------
# Compact local checks
# ---------------------------------------------------------------------------

# Fast inner loop: fmt + clippy + test — run `just gate` before pushing.
[doc("Run the fast formatting, Clippy, and test loop.")]
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

# Scoped check for one crate, e.g. `just check kamu-iso3166`.
[doc("Clippy and test one root-workspace crate.")]
check crate:
    #!/usr/bin/env bash
    set -uo pipefail
    fail=0
    cargo clippy -p '{{ crate }}' --all-targets --message-format=short -- -D warnings -D clippy::all || fail=1
    cargo nextest run -p '{{ crate }}' || fail=1
    cargo test -p '{{ crate }}' --doc --quiet || fail=1
    exit "$fail"

# Nextest plus the doctests it cannot run. Missing nextest fails rather than
# changing process-isolation semantics through a fallback runner.
[doc("Run root-workspace nextest and doctests.")]
test-fast:
    #!/usr/bin/env bash
    set -uo pipefail
    cargo nextest run --workspace
    cargo test --workspace --doc --quiet
