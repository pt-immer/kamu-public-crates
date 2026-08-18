//! Classify changed paths for CI, and reject unknown repository surfaces.
//!
//! A path no class owns is a surface no job checks, so classification fails closed rather than
//! returning an empty set.

use std::collections::{BTreeMap, BTreeSet};

pub const BASE_CLASSES: [&str; 9] =
    ["docs", "iso3166", "logging", "money", "moneypg", "shared", "shell", "snap", "tools"];

/// A change to any of these bears on every crate: the lockfile and root manifest resolve them
/// all, the Justfile defines every recipe, the workflows define every job, and `scripts/` holds
/// this classifier.
///
/// Routing `scripts/test_*.py` somewhere narrower was considered and refused. It would be sound —
/// the `changes` job runs the classifier and never its tests — but it buys a class of its own, and
/// therefore one more edge to justify, for edits that almost always accompany a change to the
/// script beside them.
const SHARED_FILES: [&str; 17] = [
    ".editorconfig",
    ".gitignore",
    ".gitmodules",
    ".mcp.json",
    "Cargo.lock",
    "Cargo.toml",
    "Justfile",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "clippy.toml",
    "deny.toml",
    "package-lock.json",
    "package.json",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "taplo.toml",
    "typos.toml",
];

const SHARED_PREFIXES: [&str; 6] =
    [".cargo/", ".config/", ".fso-amem/", ".github/actions/", ".github/workflows/", "scripts/"];

const SHARED_GITHUB_FILES: [&str; 2] = [".github/CODEOWNERS", ".github/dependabot.yml"];

const DOC_CONFIG_FILES: [&str; 3] = [".markdownlint-cli2.jsonc", "taplo.toml", "typos.toml"];

/// One class a workflow job gates on, the base classes that select it, and why working on those
/// selects it.
///
/// The reason is a required field, so a class cannot be added without answering why a change to
/// A runs B's jobs.
pub struct Derived {
    pub name: &'static str,
    pub sources: &'static [&'static str],
    pub reason: &'static str,
}

/// No entry is narrowed by inspecting diff content. This module receives paths, not hunks; a job
/// missed by a content heuristic fails quietly, and the fail-closed direction is the one that
/// cannot certify an unproven change.
pub const DERIVED_CLASSES: [Derived; 9] = [
    Derived {
        name: "rust",
        sources: &["iso3166", "logging", "money", "snap", "shared", "tools"],
        reason: "fmt, workspace Clippy and the workspace test job resolve every member in one \
                 graph, so any member's source is an input to all of them; `tools/` is a member \
                 like any other, and its tests read files rather than running any lane container",
    },
    Derived { name: "iso", sources: &["iso3166", "shared"], reason: "the kamu-iso3166 jobs" },
    Derived { name: "log", sources: &["logging", "shared"], reason: "the kamu-logging jobs" },
    Derived { name: "money", sources: &["money", "shared"], reason: "the kamu-money-core jobs" },
    Derived {
        name: "snap",
        sources: &["snap", "shared"],
        reason: "one class for six crates: they depend on each other, so testing one without the \
                 others proves less than it appears to",
    },
    Derived {
        name: "moneypg",
        sources: &["moneypg", "shared"],
        reason: "the excluded lane patches kamu-money-core to a local path and compiles it, so \
                 that crate's package inputs are inputs to this lane as well",
    },
    Derived {
        name: "worker",
        sources: &["logging", "shared"],
        reason: "the Cloudflare Worker example is a separate workspace whose only first-party \
                 dependency is kamu-logging's wasm feature set",
    },
    Derived {
        name: "lint",
        sources: &BASE_CLASSES,
        reason: "formatting, spelling, Markdown and TOML checks read files rather than crates, so \
                 every classified change is in scope for them",
    },
    Derived {
        name: "shell",
        sources: &["shell"],
        reason: "deliberately without `shared`: ShellCheck reads exactly the .sh files this class \
                 already tracks, so a Justfile or workflow edit changes nothing it looks at",
    },
];

/// Paths that no class owns.
#[derive(Debug)]
pub struct Unclassified(pub Vec<String>);

impl std::fmt::Display for Unclassified {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "CI path classification is incomplete. Add an explicit owner for:")?;
        for path in &self.0 {
            writeln!(formatter, "  - {path}")?;
        }
        Ok(())
    }
}

/// Every CI class owning one repository-relative path.
pub fn classify_path(path: &str) -> BTreeSet<&'static str> {
    let mut classes = BTreeSet::new();

    if path.starts_with("crates/iso3166/") {
        classes.insert("iso3166");
    } else if path.starts_with("crates/logging/") {
        classes.insert("logging");
    } else if let Some(relative) = path.strip_prefix("crates/money-core/") {
        classes.insert("money");
        // Unit tests live inline under `src/`, and a dependency's `#[cfg(test)]` code is never
        // compiled, so a test-only edit selects a lane it cannot affect. That over-selection is
        // deliberate: this function receives paths, not diffs.
        if matches!(relative, "Cargo.toml" | "build.rs")
            || relative.starts_with("build/")
            || relative.starts_with("src/")
            || relative.starts_with("vendor/")
        {
            classes.insert("moneypg");
        }
    } else if path.starts_with("crates/snap-") {
        classes.insert("snap");
    } else if path.starts_with("extensions/money-pg/") {
        classes.insert("moneypg");
    } else if path.starts_with("tools/") {
        classes.insert("tools");
    }

    if path.ends_with(".md") || DOC_CONFIG_FILES.contains(&path) {
        classes.insert("docs");
    }
    if path.ends_with(".sh") {
        classes.insert("shell");
    }
    if SHARED_FILES.contains(&path)
        || SHARED_GITHUB_FILES.contains(&path)
        || SHARED_PREFIXES.iter().any(|prefix| path.starts_with(prefix))
    {
        classes.insert("shared");
    }

    classes
}

/// Classify paths, failing when any path has no declared owner.
pub fn classify_paths<I, S>(paths: I) -> Result<BTreeMap<&'static str, bool>, Unclassified>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let unique: BTreeSet<String> =
        paths.into_iter().map(|path| path.as_ref().to_owned()).filter(|path| !path.is_empty()).collect();

    let mut base: BTreeMap<&'static str, bool> = BASE_CLASSES.iter().map(|name| (*name, false)).collect();
    let mut unclassified = Vec::new();

    for path in &unique {
        let owned = classify_path(path);
        if owned.is_empty() {
            unclassified.push(path.clone());
        }
        for name in owned {
            base.insert(name, true);
        }
    }

    if !unclassified.is_empty() {
        return Err(Unclassified(unclassified));
    }

    // Derived values read the base snapshot, because some of them reuse a base class name and
    // would otherwise consume a value they had just replaced.
    let mut classes = base.clone();
    for derived in &DERIVED_CLASSES {
        let fired = derived.sources.iter().any(|source| base[source]);
        classes.insert(derived.name, fired);
    }
    Ok(classes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(path: &str) -> BTreeSet<&'static str> {
        classify_path(path)
    }

    #[test]
    fn each_crate_family_has_an_owner() {
        for (path, expected) in [
            ("crates/iso3166/src/lib.rs", vec!["iso3166"]),
            ("crates/logging/src/lib.rs", vec!["logging"]),
            ("crates/money-core/src/lib.rs", vec!["money", "moneypg"]),
            ("crates/snap-response/src/lib.rs", vec!["snap"]),
            ("extensions/money-pg/Cargo.toml", vec!["moneypg"]),
        ] {
            let classes = owned(path);
            for name in expected {
                assert!(classes.contains(name), "{path} should select {name}");
            }
        }
    }

    #[test]
    fn money_core_package_inputs_retest_the_extension() {
        for path in [
            "crates/money-core/Cargo.toml",
            "crates/money-core/build.rs",
            "crates/money-core/build/iso4217.rs",
            "crates/money-core/src/arithmetic/kernel/add_sub.rs",
            "crates/money-core/vendor/list-one.xml",
        ] {
            let classes = classify_paths([path]).expect("every fixture is owned");
            assert!(classes["money"], "{path} selects money");
            assert!(classes["moneypg"], "{path} selects moneypg");
        }
        assert!(
            !owned("crates/money-core/README.md").contains("moneypg"),
            "documentation does not change the extension's compiled dependency"
        );
    }

    #[test]
    fn root_policy_files_are_shared() {
        for path in [
            ".gitignore",
            ".gitmodules",
            ".github/CODEOWNERS",
            "LICENSE-APACHE",
            "Cargo.toml",
            "rust-toolchain.toml",
        ] {
            assert!(owned(path).contains("shared"), "{path} is shared");
        }
    }

    #[test]
    fn docs_symlinks_and_submodule_paths_are_owned() {
        assert!(owned("CLAUDE.md").contains("docs"));
        assert!(owned(".github/copilot-instructions.md").contains("docs"));
        assert!(owned("crates/iso3166/vendor/iso3166-csv").contains("iso3166"));
    }

    #[test]
    fn shell_ownership_follows_extension_not_directory() {
        assert_eq!(BTreeSet::from(["shell"]), owned("ops/new-check.sh"));
        let classes = classify_paths(["ops/new-check.sh"]).expect("owned");
        assert!(classes["shell"]);
        assert!(classes["lint"]);
    }

    #[test]
    fn crate_markdown_runs_crate_and_docs_checks() {
        assert_eq!(BTreeSet::from(["docs", "logging"]), owned("crates/logging/README.md"));
    }

    #[test]
    fn an_unknown_directory_fails_closed() {
        let error = classify_paths(["scaffolds/new/config.yml"]).expect_err("no owner");
        assert!(error.to_string().contains("scaffolds/new/config.yml"));
    }

    #[test]
    fn a_shared_change_fans_out_to_exactly_the_classes_that_list_it() {
        let classes = classify_paths(["Cargo.lock"]).expect("owned");
        for derived in &DERIVED_CLASSES {
            assert_eq!(
                derived.sources.contains(&"shared"),
                classes[derived.name],
                "{} fires on a shared change iff it lists shared",
                derived.name
            );
        }
    }

    #[test]
    fn root_documentation_alone_stays_documentation_alone() {
        let classes = classify_paths(["README.md"]).expect("owned");
        assert!(classes["docs"]);
        assert!(classes["lint"]);
        assert!(!classes["rust"]);
        assert!(!classes["worker"]);
    }
}
