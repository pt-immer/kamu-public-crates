set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes
default:
    @just --list

# Format every crate
fmt:
    cargo fmt --all

# Check formatting (CI)
fmt-check:
    cargo fmt --all --check

# Lint: workspace clippy::all + kamu-iso3166 pedantic
clippy:
    cargo clippy --workspace --all-targets -- -D warnings -D clippy::all
    cargo clippy -p kamu-iso3166 --all-features --all-targets -- -D clippy::pedantic

# Test the workspace plus kamu-iso3166 feature permutations
test:
    cargo test --workspace
    cargo test -p kamu-iso3166 --all-features
    cargo test -p kamu-iso3166 --no-default-features --features serde

# Build docs the way docs.rs does
doc:
    RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps
    RUSTDOCFLAGS=-Dwarnings cargo doc -p kamu-iso3166 --no-deps --all-features

# Supply-chain audit
deny:
    cargo deny check

# Coverage gate for kamu-iso3166
cov:
    cargo llvm-cov -p kamu-iso3166 --all-features --ignore-filename-regex 'generated|build/' --fail-under-lines 95

# no_std cross-compile (needs: rustup target add thumbv7em-none-eabi)
nostd:
    cargo build -p kamu-iso3166 --no-default-features --target thumbv7em-none-eabi

# Dry-run publish a single crate, e.g. `just publish-dry kamu-iso3166`
publish-dry crate:
    cargo publish -p {{ crate }} --dry-run

# Initialize the vendored ISO 3166 data submodule
submodules:
    git submodule update --init --recursive

# Run everything the PR pipeline runs
ci: fmt-check clippy test doc deny nostd
