//! What the public-workspace gate schedules.

use repo_policy::dev_env::load_manifest;
use repo_policy::gate::{Group, stages};
use repo_policy::justfile::recipes;
use repo_policy::repo_root;

fn scheduled() -> Vec<String> {
    let root = repo_root();
    let manifest = load_manifest(&root);
    stages(&manifest.rust.msrv).into_iter().map(|stage| stage.command).collect()
}

#[test]
fn the_gate_runs_the_repository_wide_linters() {
    let commands = scheduled();
    for required in ["just lint-all", "just deny"] {
        assert!(commands.iter().any(|command| command == required), "the gate does not run {required}");
    }
}

/// Every stage names a recipe that exists, except the MSRV stage, which drives cargo directly at
/// a channel no recipe pins.
#[test]
fn every_scheduled_recipe_exists() {
    let recipes = recipes(&repo_root());
    for command in scheduled() {
        let Some(name) = command.strip_prefix("just ") else {
            continue;
        };
        assert!(recipes.contains_key(name), "the gate schedules `just {name}`, which is not a recipe");
    }
}

/// A stage in the wrong group either serialises against a lock it does not share or contends for
/// one it does. Both are silent: the run is slower or flakier, never wrong.
#[test]
fn only_the_coverage_stage_drives_the_coverage_target_directory() {
    for stage in stages("1.0.0") {
        if stage.command.contains("cov") {
            assert_eq!(Group::Cov, stage.group, "{} drives coverage outside the cov group", stage.name);
        }
    }
}
