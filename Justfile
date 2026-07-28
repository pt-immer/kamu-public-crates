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

# kamu-money-core denies pedantic, cargo and nine named restriction lints in its
# own lib.rs, which is stricter than the workspace — arithmetic that must not be
# wrong earns a tighter setting than framework glue does.
# Clippy kamu-money-core's feature permutations (stricter than the workspace).
clippy-money:
    cargo clippy -p kamu-money-core --all-features --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-money-core --no-default-features -- -D warnings -D clippy::all

# Lint Rust: workspace clippy::all + iso3166 pedantic + ALL non-default feature perms
lint-rust: clippy-workspace clippy-iso3166 clippy-logging clippy-money clippy-snap

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
    # THE MACHINE'S OWN NAMES, read from the environment so they are never written
    # into this file. Generic container and CI account names are skipped for the
    # same reason the home-path scan excludes them: `runner` is not anybody's
    # identity, and on a GitHub Actions runner $USER *is* `runner` — which made
    # this recipe fail CI on the words "test runner" and "AsyncRunner" while
    # passing on every developer machine whose username does not collide.
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

# Lint tracked shell scripts. Shell is part of the correctness boundary in the PG
# lane, so this is a real gate rather than a formality.
#
# `-x` FOLLOWS `source`d FILES, which is what makes cluster.sh and artifact.sh
# checkable at all -- without it they are opaque and SC1091 fires on every one.
#
# THE LANE IS CHECKED FROM ITS OWN ROOT, and that is not a stylistic choice. Its
# scripts resolve siblings relative to the lane root (`kamu-money-pg/yb/...`),
# exactly as they did in the repository they came from -- which is the same
# property that let 37 of them transplant without a single edit. Checked from
# here instead, every one of those paths resolves to nothing and shellcheck
# reports 33 unfollowable sources. Measured both ways: 33 findings from the
# repository root, 0 from the lane root.
lint-shell:
    #!/usr/bin/env bash
    set -uo pipefail
    if ! command -v shellcheck >/dev/null 2>&1; then
        echo "lint-shell: shellcheck NOT INSTALLED -- run 'just setup'. Failing rather than passing vacuously." >&2
        exit 1
    fi
    lane=extensions/money-pg
    rc=0
    host=$(git ls-files '*.sh' | grep -v "^$lane/" || true)
    lane_files=$(git ls-files "$lane/*.sh" | sed "s|^$lane/||" || true)
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

# Test kamu-money-core WITHOUT Docker: every feature permutation whose tests do
# not start a container. This is the recipe `just gate` reaches through test-all.
# Test kamu-money-core's Docker-free feature permutations + its doctests.
test-money:
    cargo nextest run -p kamu-money-core
    cargo nextest run -p kamu-money-core --features serde
    cargo nextest run -p kamu-money-core --features postgres -E 'not binary(pg_roundtrip)'
    # nextest cannot run doctests.
    cargo test -p kamu-money-core --all-features --doc --quiet

# Test kamu-money-core's container-backed suites. REQUIRES a reachable Docker
# daemon. Concurrency is bounded by the `money-db` test group in
# .config/nextest.toml, not by a flag here, so every invocation agrees.
#
# Deliberately NOT in `gate`: the gate must stay runnable without Docker, and a
# gate stage that cannot run is a stage that gets skipped. `pg_native_column` and
# `yugabyte_roundtrip` are excluded too — they need the native kmoney extension,
# which belongs to the PostgreSQL lane rather than to this crate's own suite.
# Test kamu-money-core's container-backed suites (REQUIRES Docker; not in gate).
test-money-db:
    cargo nextest run -p kamu-money-core --all-features -E 'binary(pg_roundtrip) or binary(sqlx_roundtrip)'

# Test the workspace plus kamu-iso3166 / kamu-snap-* feature permutations.
# Every test binary runs under cargo-nextest (process-per-test isolation, see
# .config/nextest.toml); doctests are a SEPARATE `cargo test --doc` pass because
# nextest does not run them.
test-all:
    # `compile_fail` is EXCLUDED here and owned by `just test-money`. Its trybuild
    # goldens are byte-exact rustc diagnostics, so they can only ever match ONE
    # compiler — and this recipe runs under both `stable` and the MSRV toolchain
    # (CI's test matrix, and `just gate`'s msrv stage). Blessed on stable, they
    # fail on 1.94 for a reason that says nothing about the code.
    cargo nextest run --workspace -E 'not binary(compile_fail)'
    # kamu-logging OTLP path (a default run exercises systemd+actix, not OTLP):
    # covers BatchSpanProcessor + the drain helpers + the runtime test.
    cargo nextest run -p kamu-logging --features with-otlp
    cargo nextest run -p kamu-iso3166 --all-features
    cargo nextest run -p kamu-iso3166 --no-default-features --features serde
    cargo nextest run -p kamu-snap-crypto --all-features
    cargo nextest run -p kamu-snap-response --all-features
    # kamu-money-core's adapters are all feature-gated, so a default run reaches
    # none of them. Container-backed and native-extension suites are excluded —
    # `just test-money-db` owns the first, the PostgreSQL lane owns the second.
    cargo nextest run -p kamu-money-core --all-features -E 'not (binary(compile_fail) or binary(pg_roundtrip) or binary(sqlx_roundtrip) or binary(pg_native_column) or binary(yugabyte_roundtrip))'
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

# Build kamu-money-core docs with all features. Every adapter is feature-gated,
# so a default-feature docs build would not render wire, pg or sqlx_pg at all
# (docs.rs parity — the manifest sets all-features there for the same reason).
# Build kamu-money-core docs with all features (CI `docs (kamu-money-core)` job).
doc-money:
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-money-core --no-deps --all-features

# Build docs the way docs.rs does
doc: doc-workspace doc-iso3166 doc-money doc-snap-response

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

# Coverage gate for kamu-snap-response. Floor 85, measured 90.11% lines on
# 2026-07-28 (687 regions, 465 lines). The five-point margin absorbs LLVM
# instrumentation drift. Framework adapters are behavior-tested by the workspace
# suite but intentionally not percentage-gated.
cov-snap-response:
    cargo llvm-cov nextest -p kamu-snap-response --all-features --fail-under-lines 85

# Coverage gate for kamu-money-core. Floor 80, measured 84.89% lines on
# 2026-07-28 (2584 regions, 1370 lines).
#
# WHY NOT HIGHER, stated so the number is not a mystery to whoever next reads it:
# the container-backed and native-extension suites are excluded here, because a
# coverage gate that needs a Docker daemon is a gate that gets skipped. Those
# suites are exactly what exercises the two driver adapters, so `sqlx_pg.rs`
# measures 0% and `pg.rs` 12.5% in this run and pull the total down by roughly
# four points. Everything else sits between 76% and 100%. Raising this floor
# means either covering the adapters offline or moving the gate behind Docker —
# not tightening the number and hoping.
#
# `build/` is excluded for the same reason kamu-iso3166 excludes it: the code
# that generates the register is exercised by the build itself, and by
# tests/register_codegen.rs, neither of which this measurement sees.
#
# `compile_fail` is excluded too. A trybuild harness executes no library code, so
# it contributes nothing to line coverage — and including it made this recipe
# depend on `rust-src`, because the no_iterator_sum golden quotes standard-library
# source. The coverage CI job installs llvm-tools-preview, not rust-src, so the
# suite mismatched there while passing in `just test-money`, which does install
# it. Measuring the same goldens in three places bought nothing and broke one.
# Coverage gate for kamu-money-core (>= 80% lines; measured 84.89%).
cov-money:
    cargo llvm-cov nextest -p kamu-money-core --all-features -E 'not (binary(compile_fail) or binary(pg_roundtrip) or binary(sqlx_roundtrip) or binary(pg_native_column) or binary(yugabyte_roundtrip))' --ignore-filename-regex 'build/' --fail-under-lines 80

# Coverage gates for every gated crate
cov-all: cov cov-logging cov-money cov-snap-crypto cov-snap-response

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

# Build the standalone Cloudflare Worker with both lockfiles.
check-worker-example:
    cargo check --manifest-path crates/logging/examples/cloudflare-worker/Cargo.toml --target wasm32-unknown-unknown
    npm --prefix crates/logging/examples/cloudflare-worker ci --no-fund --no-audit
    npm --prefix crates/logging/examples/cloudflare-worker run build

# Test fail-closed CI classification, registry probing, and standalone-package
# ownership; then prove every tracked path is classified.
test-repo-policy:
    python3 -m unittest discover -s scripts -p 'test_*.py'
    python3 scripts/ci_paths.py check-tracked

# ---------------------------------------------------------------------------
# Publish / vendored data / housekeeping
# ---------------------------------------------------------------------------

# Dry-run publish a single crate, e.g. `just publish-dry kamu-iso3166`
publish-dry crate:
    cargo publish -p {{ crate }} --dry-run --allow-dirty

# Dry-run every publishable package reported by Cargo metadata. A new member
# enters this loop automatically; `publish = false` packages stay out.
publish-all:
    #!/usr/bin/env bash
    set -euo pipefail
    mapfile -t crates < <(cargo metadata --no-deps --format-version 1 | python3 -c \
      'import json,sys; print("\n".join(p["name"] for p in json.load(sys.stdin)["packages"] if p["publish"] != []))')
    [ "${#crates[@]}" -gt 0 ] || { echo "publish-all: metadata returned no publishable packages" >&2; exit 1; }
    for crate in "${crates[@]}"; do
      echo "publish-all: $crate"
      cargo publish -p "$crate" --dry-run --allow-dirty
    done

# Initialize the vendored ISO 3166 data submodule
submodules:
    git submodule update --init --recursive

# Remove build artifacts
clean:
    cargo clean

# ---------------------------------------------------------------------------
# Aggregates
# ---------------------------------------------------------------------------

# Published-crate local gate. Runs lint, tests, MSRV, coverage, docs, cross
# builds, the standalone Worker, repository policy, and the root dependency
# audit as compact PASS/FAIL lines; full output for failed stages,
# or everything with `VERBOSE=1 just gate`. There is NO silent skip: a missing
# tool or target (taplo, typos, markdownlint, cargo-llvm-cov, the 1.94 toolchain,
# the wasm32 / thumbv7em targets) makes its stage FAIL loudly — run `just setup`
# (and `rustup toolchain install 1.94`) first. `just check-all` is the fast loop.
gate:
    #!/usr/bin/env bash
    set -uo pipefail
    names=("lint-all" "test-all" "test-money" "test-repo-policy" "msrv(1.94)" "cov-all" "doc" "build-nostd" "build-wasm" "build-wasm-snap" "check-worker-example" "deny")
    cmds=("just lint-all"
          "just test-all"
          "just test-money"
          "just test-repo-policy"
          "cargo +1.94 nextest run --workspace -E 'not binary(compile_fail)' && cargo +1.94 test --workspace --doc --quiet"
          "just cov-all"
          "just doc"
          "just build-nostd"
          "just build-wasm"
          "just build-wasm-snap"
          "just check-worker-example"
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
    # STATED NON-COVERAGE, WHICH IS NOT THE SAME AS A SILENT SKIP. This gate deliberately does not
    # build the PostgreSQL extension lane -- that needs Docker and takes hours, and a gate nobody
    # can afford to run before a push is a gate that stops being run. But a green PASS beside
    # uncommitted extension changes reads as "all clear" unless it says otherwise, so it says so.
    if [ -n "$(git status --porcelain --untracked-files=all -- extensions/money-pg)" ]; then
      echo
      echo "  NOTE  extensions/money-pg has changes this gate did NOT cover."
      echo "        Run 'just gate-all' before pushing them."
    fi
    exit "$fail"

# Run a recipe in the PostgreSQL extension lane, e.g. `just pg gate-pg`.
# `just pg` on its own lists that lane's recipes.
#
# A PASSTHROUGH RATHER THAN A MIRROR PER RECIPE. The lane has around fifty; copying their names up
# here would create a second list to keep in step, and the copy that falls behind is the one
# somebody trusts. CI still calls `just <something>` for every job, so the Justfile-as-source-of-
# truth rule holds either way.
pg *ARGS:
    cd extensions/money-pg && just {{ ARGS }}

# The PostgreSQL lane's gate. Hours, and needs Docker -- deliberately separate from `gate`, which
# must stay fast enough to run before every push.
gate-pg:
    cd extensions/money-pg && just gate-pg

# Everything: the nine published crates AND the extension lane. The pre-push barrier for a change
# that touches extensions/money-pg.
gate-all: gate gate-pg

# Local published-crate gate plus metadata-derived publish dry-runs.
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
