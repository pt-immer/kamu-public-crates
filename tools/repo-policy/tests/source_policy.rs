//! Persisted money hashes use `kamu_money_core::advanced::stable_hash`.
//!
//! `DefaultHasher` output is not stable across Rust releases, so a value hashed with it and
//! written down cannot be re-derived by a later toolchain.

use std::fs;
use std::process::Command;

use repo_policy::source_policy::unstable_hasher_offences;
use repo_policy::{repo_root, tracked};

#[test]
fn no_tracked_rust_constructs_an_unstable_hasher() {
    let offences = unstable_hasher_offences(&repo_root());
    assert!(
        offences.is_empty(),
        "use kamu_money_core::advanced::stable_hash for persisted values: {offences:?}"
    );
}

#[test]
fn the_scan_reaches_the_whole_repository_including_the_excluded_lane() {
    let files = tracked(&["*.rs"]);
    assert!(files.len() > 50, "tracked Rust source discovery found too few files: {}", files.len());
    assert!(
        files.iter().any(|path| path.starts_with("extensions/money-pg/")),
        "the excluded lane is outside the scan"
    );
}

#[test]
fn a_planted_violation_is_found_wherever_it_is_nested() {
    let root = std::env::temp_dir().join("repo-policy-source-policy-control");
    let _ = fs::remove_dir_all(&root);
    let nested = root.join("extensions/example/src/nested");
    fs::create_dir_all(&nested).expect("scratch tree is creatable");

    let status =
        Command::new("git").args(["init", "--quiet"]).current_dir(&root).status().expect("git init runs");
    assert!(status.success(), "git init failed");

    fs::write(
        nested.join("guard.rs"),
        "fn bad() { let _ = std::collections::hash_map::DefaultHasher::new(); }\n",
    )
    .expect("planted source is writable");
    let status = Command::new("git")
        .args(["add", "extensions/example/src/nested/guard.rs"])
        .current_dir(&root)
        .status()
        .expect("git add runs");
    assert!(status.success(), "git add failed");

    let offences = unstable_hasher_offences(&root);
    assert_eq!(1, offences.len(), "the planted violation was not found: {offences:?}");
    assert_eq!("extensions/example/src/nested/guard.rs", offences[0].file);

    fs::remove_dir_all(&root).expect("scratch tree is removable");
}
