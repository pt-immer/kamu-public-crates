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
    for spec in taplo:taplo-cli typos:typos-cli cargo-llvm-cov:cargo-llvm-cov cargo-deny:cargo-deny; do \
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

# Check formatting of every file type (Rust + TOML)
fmt-check:
    cargo fmt --all --check
    taplo fmt --check

# Auto-fix what tooling can (Markdown + spelling)
fix:
    markdownlint-cli2 --fix "**/*.md"
    typos -w

# ---------------------------------------------------------------------------
# Lint (read-only), per file type + aggregate
# ---------------------------------------------------------------------------

# Lint Rust: workspace clippy::all + kamu-iso3166 pedantic
lint-rust:
    cargo clippy --workspace --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-iso3166 --all-features --all-targets -- -D clippy::pedantic

# Lint Markdown
lint-md:
    markdownlint-cli2 "**/*.md"

# Lint TOML
lint-toml:
    taplo lint

# Spell-check sources and docs
lint-spell:
    typos

# Lint every file type (formatting + Rust + Markdown + TOML + spelling)
lint-all: fmt-check lint-rust lint-md lint-toml lint-spell

# ---------------------------------------------------------------------------
# Tests / docs / supply chain
# ---------------------------------------------------------------------------

# Test the workspace plus kamu-iso3166 / kamu-snap-* feature permutations
test-all:
    cargo test --workspace
    cargo test -p kamu-iso3166 --all-features
    cargo test -p kamu-iso3166 --no-default-features --features serde
    cargo test -p kamu-snap-crypto --all-features
    cargo test -p kamu-snap-response --all-features
    # Leaf lib must also compile with default features off (HMAC/RSA-only crypto,
    # no snap-bi/webhook). Tests pull snap-bi, so this is a lib check, not a test.
    cargo check -p kamu-snap-crypto --no-default-features

# Build docs the way docs.rs does
doc:
    RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-iso3166 --no-deps --all-features
    # kamu-snap-response gains the `crypto` feature under all-features (docs.rs
    # parity). The other snap crates' all-features == default, already covered.
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-snap-response --no-deps --all-features

# Supply-chain audit
deny:
    cargo deny check

# ---------------------------------------------------------------------------
# Coverage
# ---------------------------------------------------------------------------

# Coverage gate for kamu-iso3166
cov:
    cargo llvm-cov -p kamu-iso3166 --all-features --ignore-filename-regex 'generated|build/' --fail-under-lines 98

# Coverage gate for kamu-logging (no --all-features: systemd XOR wasm32)
cov-logging:
    cargo llvm-cov -p kamu-logging --fail-under-lines 70

# Coverage gate for kamu-snap-crypto. Floor 70 (measured ~74%): the default-on
# `webhook` providers ship without tests upstream; raising this is future work.
cov-snap-crypto:
    cargo llvm-cov -p kamu-snap-crypto --all-features --fail-under-lines 70

# Coverage gate for kamu-snap-response. Floor 70 (measured ~74%); `category.rs`
# is currently untested upstream. The 4 thin actix/axum adapter crates have no
# tests (framework-bound glue) and are intentionally compile-only, not gated.
cov-snap-response:
    cargo llvm-cov -p kamu-snap-response --all-features --fail-under-lines 70

# Coverage gates for every gated crate
cov-all: cov cov-logging cov-snap-crypto cov-snap-response

# HTML coverage report for the whole workspace; prints the output path
cov-html:
    cargo llvm-cov --workspace --ignore-filename-regex 'generated|build/' --html
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

# Dry-run publish every crate. The snap leaf crates verify standalone; the 4
# adapter crates depend on an unpublished base crate, so their dry-run uses
# --no-verify (packaging only) until the base is on crates.io. Real publishes
# go in dependency order: crypto -> response -> adapters (see on-release-published).
publish-all:
    cargo publish -p kamu-iso3166 --dry-run
    cargo publish -p kamu-logging --dry-run
    cargo publish -p kamu-snap-crypto --dry-run
    cargo publish -p kamu-snap-response --dry-run
    cargo publish -p kamu-snap-crypto-actix --dry-run --no-verify
    cargo publish -p kamu-snap-crypto-axum --dry-run --no-verify
    cargo publish -p kamu-snap-response-actix --dry-run --no-verify
    cargo publish -p kamu-snap-response-axum --dry-run --no-verify

# Initialize the vendored ISO 3166 data submodule
submodules:
    git submodule update --init --recursive

# Remove build artifacts
clean:
    cargo clean

# ---------------------------------------------------------------------------
# Aggregates
# ---------------------------------------------------------------------------

# Everything read-only: lint + tests + coverage + docs + cross builds + deny
check-all: lint-all test-all cov-all doc build-nostd build-wasm build-wasm-snap deny

# The full pipeline: everything check-all runs, plus a publish dry-run
ci: check-all publish-all
