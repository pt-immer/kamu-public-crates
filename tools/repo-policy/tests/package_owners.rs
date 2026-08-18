//! Every standalone Cargo package has gate, CI and update ownership.
//!
//! A package outside the root workspace is built by nothing unless something names it, so each
//! one declares the recipe that gates it, the job that runs that recipe, and the directories
//! Dependabot updates.

use std::collections::{BTreeMap, BTreeSet};

use repo_policy::justfile::recipes;
use repo_policy::{read, repo_root, tracked};

#[derive(Debug, serde::Deserialize)]
struct Owner {
    manifest: String,
    gate_recipe: String,
    ci_job: String,
    dependabot_directories: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct Policy {
    #[serde(default)]
    standalone: Vec<Owner>,
}

fn owners() -> BTreeMap<String, Owner> {
    let policy: Policy =
        toml::from_str(&read(".config/package-owners.toml")).expect("the owner policy parses");
    policy.standalone.into_iter().map(|owner| (owner.manifest.clone(), owner)).collect()
}

/// A manifest declaring both `[package]` and `[workspace]` is its own workspace root.
fn standalone_manifests() -> BTreeSet<String> {
    tracked(&["*Cargo.toml"])
        .into_iter()
        .filter(|relative| {
            let document: toml::Value =
                toml::from_str(&read(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"));
            document.get("package").is_some() && document.get("workspace").is_some()
        })
        .collect()
}

#[test]
fn every_standalone_package_has_an_owner_record() {
    assert_eq!(standalone_manifests(), owners().keys().cloned().collect::<BTreeSet<_>>());
}

#[test]
fn each_changelog_opens_on_the_version_its_manifest_carries() {
    let crates = repo_root().join("crates");
    let mut checked = 0_usize;
    for entry in std::fs::read_dir(&crates).expect("crates/ is readable") {
        let directory = entry.expect("directory entry is readable").path();
        let manifest = directory.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let name = directory.file_name().expect("a crate directory has a name").to_string_lossy();
        let document: toml::Value =
            toml::from_str(&std::fs::read_to_string(&manifest).expect("the manifest is readable"))
                .expect("the manifest parses");
        let version = document["package"]["version"].as_str().expect("the package declares a version");

        let changelog = std::fs::read_to_string(directory.join("CHANGELOG.md"))
            .unwrap_or_else(|_| panic!("{name} has no changelog to bind"));
        // Both heading styles in the tree are accepted, bracketed and bare: the claim is the
        // version, not the format.
        // The first heading naming a version. `## [Unreleased]` is not one, and skipping it is
        // the difference between binding the release and binding whatever sits at the top.
        let heading = changelog
            .lines()
            .filter_map(|line| line.strip_prefix("## "))
            .map(|heading| heading.trim_start_matches('['))
            .map(|heading| heading.split([']', ' ']).next().unwrap_or(heading))
            .find(|heading| heading.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or_else(|| panic!("{name} has no released version heading"));
        assert_eq!(version, heading, "{name}: the changelog opens on another version");
        checked += 1;
    }
    assert!(checked > 0, "no crate changelog was checked; this would pass vacuously");
}

#[test]
fn owner_records_name_real_gate_recipes_and_ci_jobs() {
    let recipes = recipes(&repo_root());
    let gate = recipes["gate"].body();
    let workflow = read(".github/workflows/on-pr-synced.yml");

    for (manifest, owner) in owners() {
        assert!(
            recipes.contains_key(&owner.gate_recipe),
            "{manifest}: no recipe named {}",
            owner.gate_recipe
        );
        assert!(
            gate.contains(&format!("\"just {}\"", owner.gate_recipe)),
            "{manifest}: the gate does not schedule {}",
            owner.gate_recipe
        );
        let job = format!("\n  {}:\n", owner.ci_job);
        let body = workflow
            .split(&job)
            .nth(1)
            .unwrap_or_else(|| panic!("{manifest}: no CI job named {}", owner.ci_job));
        assert!(
            body.contains(&format!("run: just {}", owner.gate_recipe)),
            "{manifest}: job {} does not run {}",
            owner.ci_job,
            owner.gate_recipe
        );
    }
}

#[test]
fn owner_records_name_dependabot_directories() {
    let dependabot = read(".github/dependabot.yml");
    for (manifest, owner) in owners() {
        assert!(
            !owner.dependabot_directories.is_empty(),
            "{manifest}: a package nothing updates is a package nothing patches"
        );
        for directory in owner.dependabot_directories {
            assert!(
                dependabot.contains(&format!("directory: \"{directory}\"")),
                "{manifest}: dependabot does not update {directory}"
            );
        }
    }
}

/// The PostgreSQL `kamu-money-core` is proven against must be one the extension lane supports.
///
/// Two different claims — what the crate is tested on, and what the extension builds for — that
/// must agree. `testcontainers`' own default answered the first with a major past end of life.
#[test]
fn the_roundtrip_postgresql_is_a_major_the_lane_supports() {
    let reference = repo_policy::justfile::variable(&repo_root(), "PG_ROUNDTRIP_IMAGE");
    let tag = reference.rsplit_once(':').expect("the image names a tag").1;
    let major = tag.split('-').next().expect("the tag opens on a major");

    let lane = repo_root().join("extensions/money-pg");
    let supported = repo_policy::justfile::variable(&lane, "PG_MAJORS");
    let majors: Vec<&str> = supported.split_whitespace().collect();
    assert!(!majors.is_empty(), "the lane declares no PostgreSQL majors");
    assert!(
        majors.contains(&major),
        "the roundtrip runs on PostgreSQL {major}, which the lane does not build for: {majors:?}"
    );
}
