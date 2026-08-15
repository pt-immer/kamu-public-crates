//! The lane pins two toolchains, and each is ONE fact spelled in many places: pgrx across
//! manifests, Dockerfiles, the tool manifest and CI, and Rust across `rust-toolchain.toml`
//! and three container images.
//!
//! The two disagree differently, which is why both are here.
//!
//! `cargo-pgrx` refuses to build an extension whose `pgrx` dependency differs from the CLI's
//! own version, because the SQL it generates and the FFI shims it links are versioned
//! together. A site left on the old number is not a cosmetic lag: it is a build that fails,
//! wherever that site is read -- inside a container, on a CI runner, or on the machine of
//! whoever runs `just setup` next.
//!
//! Rust fails the other way, silently. `rust-toolchain.toml` wins over whatever a base image
//! ships, so the compiler is always the pinned one and the tests are always right. What a
//! mismatch costs is a second toolchain downloaded inside every container, on every run,
//! without ever producing a wrong answer to notice it by. It went unseen until a layer
//! measurement asked why the image had grown by 305 MB.
//!
//! Every expectation below is DERIVED from the pin it checks, so this file holds no version
//! number and cannot become the second list it exists to prevent. What it does hold is the
//! list of PLACES, each asserted through an anchor that must be present -- so a renamed
//! directive or a moved file fails loudly rather than passing because it matched nothing.

mod support;

use std::path::Path;

/// The authoritative statement of which pgrx this lane builds against.
///
/// It is exact (`=x.y.z`) rather than a caret range because `cargo-pgrx` is installed by
/// version and must match: a range would let the lockfile pick a pgrx the installed CLI
/// refuses, which resolves fine and then fails at `cargo pgrx schema`.
fn pinned_version() -> String {
    let manifest = extension_manifest();
    let requirement = manifest
        .get("dependencies")
        .and_then(|table| table.get("pgrx"))
        .and_then(toml::Value::as_str)
        .expect("kamu-money-pg must depend on pgrx by a plain version requirement");
    requirement
        .strip_prefix('=')
        .unwrap_or_else(|| {
            panic!("pgrx must be pinned exactly for cargo-pgrx to match it, not `{requirement}`")
        })
        .to_owned()
}

fn extension_manifest() -> toml::Value {
    support::manifest(support::lane_root().join("kamu-money-pg/Cargo.toml"))
}

/// Assert that every line naming `anchor` also names `expected`.
///
/// The anchor is a positive control. Asserting only "the file contains the new version"
/// would pass a file that gained the version somewhere while leaving a second, stale
/// install line behind -- which is exactly how the CI workflow carries two `cargo-pgrx@`
/// pins and how a `Dockerfile.pg15` sits beside a `Dockerfile`.
fn every_anchored_line_carries(path: &Path, anchor: &str, expected: &str) {
    let contents = support::read(path);
    let anchored: Vec<&str> = contents.lines().filter(|line| line.contains(anchor)).collect();
    assert!(
        !anchored.is_empty(),
        "{} no longer contains `{anchor}` -- this guard would pass vacuously; re-point it",
        path.display()
    );
    for line in anchored {
        assert!(line.contains(expected), "{}: `{}` must carry `{expected}`", path.display(), line.trim());
    }
}

#[test]
fn every_pgrx_crate_takes_the_pinned_version_exactly() {
    let exact = format!("={}", pinned_version());
    let manifest = extension_manifest();

    // pgrx-bench and pgrx-tests are compiled against pgrx's internals. A minor-version
    // disagreement between them is not a resolution conflict -- Cargo is happy to take
    // two -- it is a link error at test or bench time.
    for (table, crate_name) in
        [("dependencies", "pgrx"), ("dependencies", "pgrx-bench"), ("dev-dependencies", "pgrx-tests")]
    {
        let entry = manifest
            .get(table)
            .and_then(|section| section.get(crate_name))
            .unwrap_or_else(|| panic!("kamu-money-pg must declare {table}.{crate_name}"));
        let requirement = entry
            .as_str()
            .or_else(|| entry.get("version").and_then(toml::Value::as_str))
            .unwrap_or_else(|| panic!("{table}.{crate_name} must carry a version requirement"));
        assert_eq!(
            requirement, exact,
            "{table}.{crate_name} must take the same exact pgrx version as the extension"
        );
    }
}

#[test]
fn the_fork_patch_names_an_immutable_tag_built_from_the_pinned_version() {
    let expected_prefix = format!("v{}-yb.", pinned_version());
    let lane = support::manifest(support::lane_root().join("Cargo.toml"));
    let patch = lane
        .get("patch")
        .and_then(|table| table.get("crates-io"))
        .expect("the lane must patch crates-io with the YugabyteDB pgrx fork");

    // BOTH crates, because patching only `pgrx` leaves `pgrx-pg-sys` resolving from the
    // registry and the build then fails at link time rather than at resolution.
    for crate_name in ["pgrx", "pgrx-pg-sys"] {
        let entry =
            patch.get(crate_name).unwrap_or_else(|| panic!("[patch.crates-io] must patch {crate_name}"));
        // A `rev`, a `branch` or a `path` all resolve and all build. None of them is a
        // release configuration: a branch moves under the lockfile, and a path cannot be
        // read from inside a container at all. The tag is the only form that names the
        // same source everywhere the lane is built.
        let tag = entry
            .get("tag")
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("{crate_name} must be patched to a TAG, never a branch, rev or path"));
        assert!(
            tag.starts_with(&expected_prefix),
            "{crate_name} is patched to `{tag}`, which is not a `{expected_prefix}*` release \
             of the fork -- the fork's crates declare their upstream version, so a tag from \
             another base would install a pgrx cargo-pgrx refuses"
        );
    }
}

#[test]
fn every_cargo_pgrx_installation_pins_the_same_version() {
    let version = pinned_version();
    let lane = support::lane_root();
    let repository = support::repository_root();

    // The PostgreSQL image builds the CLI from an ARG so that the version is visible in
    // one place per file; the YugabyteDB images name it inline.
    every_anchored_line_carries(
        &lane.join("kamu-money-pg/Dockerfile"),
        "ARG PGRX_VERSION=",
        &format!("ARG PGRX_VERSION={version}"),
    );
    for image in ["kamu-money-pg/yb/Dockerfile", "kamu-money-pg/yb/Dockerfile.pg15"] {
        every_anchored_line_carries(
            &lane.join(image),
            "cargo install cargo-pgrx",
            &format!("--version {version}"),
        );
    }

    // CI installs a prebuilt CLI and caches PGRX_HOME beside it. The cache key carries the
    // version because a PGRX_HOME initialised by one cargo-pgrx is not interchangeable
    // with another's -- reusing it across a bump is a silently wrong toolchain, not a
    // slow build.
    let workflow = repository.join(".github/workflows/on-pr-synced.yml");
    every_anchored_line_carries(&workflow, "cargo-pgrx@", &format!("cargo-pgrx@{version}"));
    every_anchored_line_carries(&workflow, "key: pgrx-", &format!("key: pgrx-{version}-"));
}

/// The lane's Rust toolchain, from the file rustup itself obeys.
fn pinned_toolchain() -> String {
    let path = support::lane_root().join("rust-toolchain.toml");
    let pin = support::manifest(&path);
    pin.get("toolchain")
        .and_then(|table| table.get("channel"))
        .and_then(toml::Value::as_str)
        .unwrap_or_else(|| panic!("{} must pin a channel", path.display()))
        .to_owned()
}

#[test]
fn every_container_starts_from_the_pinned_rust_toolchain() {
    let channel = pinned_toolchain();
    let lane = support::lane_root();

    // An EXACT patch version, not the `1.96` series tag. The series tag floats to the newest
    // patch, and rustup then honours `rust-toolchain.toml` by downloading a second toolchain
    // inside the container -- correct, invisible, and paid on every run.
    assert!(
        channel.split('.').count() == 3,
        "the channel must be an exact patch version for a base image to match it, not `{channel}`"
    );
    every_anchored_line_carries(
        &lane.join("kamu-money-pg/Dockerfile"),
        "ARG RUST_VERSION=",
        &format!("ARG RUST_VERSION={channel}"),
    );

    // The YugabyteDB images have no Rust base to inherit, so they install rustup themselves and
    // name the toolchain on the command line.
    for image in ["kamu-money-pg/yb/Dockerfile", "kamu-money-pg/yb/Dockerfile.pg15"] {
        every_anchored_line_carries(
            &lane.join(image),
            "--default-toolchain",
            &format!("--default-toolchain {channel}"),
        );
    }
}

#[test]
fn clippy_lints_against_the_pinned_toolchain() {
    // Clippy decides which lints apply from this, so a stale value lints the
    // lane against a Rust it no longer builds with. It takes the series rather
    // than the exact patch every other pin here wants.
    let channel = pinned_toolchain();
    let series = channel.split('.').take(2).collect::<Vec<_>>().join(".");

    let path = support::lane_root().join("clippy.toml");
    let declared = support::manifest(&path)
        .get("msrv")
        .and_then(toml::Value::as_str)
        .unwrap_or_else(|| panic!("{} must pin an msrv", path.display()))
        .to_owned();

    assert_eq!(
        series,
        declared,
        "{} lints against {declared} while rustup builds with {channel}",
        path.display()
    );
}

#[test]
fn the_tool_manifest_installs_the_pinned_cargo_pgrx() {
    let version = pinned_version();
    let path = support::repository_root().join(".config/dev-tools.json");
    let manifest: serde_json::Value = serde_json::from_str(&support::read(&path))
        .unwrap_or_else(|error| panic!("{} must parse as JSON: {error}", path.display()));

    // `just setup` and `just doctor` read this file, so a stale entry here is the one
    // that reaches a developer's machine rather than a container.
    let declared = manifest
        .get("ci_only_tools")
        .and_then(|tools| tools.get("cargo-pgrx"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("{} must declare cargo-pgrx", path.display()));
    assert_eq!(
        declared,
        version,
        "{} installs cargo-pgrx {declared} against a lane pinned to pgrx {version}",
        path.display()
    );
}
