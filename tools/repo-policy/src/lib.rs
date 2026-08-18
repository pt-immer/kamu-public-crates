//! Repository policy: one decoder per repository artifact, and the checks that read them.
//!
//! Every pinned version this repository depends on is stated once, in
//! `.config/dev-tools.json`, and reached from here.

use std::path::{Path, PathBuf};

pub mod actions;
pub mod ci_paths;
pub mod dev_env;
pub mod dev_tools;
pub mod gate;
pub mod justfile;
pub mod registry;
pub mod source_policy;

/// The repository root, from this crate's own location.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two levels below the repository root")
        .to_path_buf()
}

/// Read a repository file, naming it rather than the absolute path when it cannot be read.
pub fn read(relative: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("cannot read {relative}: {error}"))
}

/// Every path git tracks matching any pattern.
///
/// Enumerating from the repository is what keeps a scan total: a list of files, or of globs
/// narrower than the thing being scanned, only covers what someone remembered.
pub fn tracked(patterns: &[&str]) -> Vec<String> {
    tracked_in(&repo_root(), patterns)
}

/// Every path git tracks under one root. Separate so a check can be pointed at a planted tree
/// and observed failing, which is the only way to know it can fail at all.
pub fn tracked_in(root: &Path, patterns: &[&str]) -> Vec<String> {
    let mut arguments = vec!["ls-files", "-z", "--"];
    arguments.extend_from_slice(patterns);
    let output = std::process::Command::new("git")
        .args(&arguments)
        .current_dir(root)
        .output()
        .expect("git ls-files runs");
    assert!(output.status.success(), "git ls-files failed for {patterns:?}");
    let listed: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!listed.is_empty(), "git tracks nothing matching {patterns:?}");
    listed
}
