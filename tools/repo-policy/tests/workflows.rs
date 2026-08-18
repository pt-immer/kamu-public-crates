//! Workflow reachability and toolchain policy.
//!
//! Supply-chain pinning is asserted in `actions.rs`; these are the claims about which jobs run,
//! what they install, and what a skipped dependency does to a job its path condition selected.

use std::collections::{BTreeMap, BTreeSet};

use repo_policy::actions::{Executable, MANIFEST_ACTION, MANIFEST_OUTPUT, job_policies, sources};
use repo_policy::ci_paths::{DERIVED_CLASSES, classify_paths};
use repo_policy::justfile::{lane_entry_recipes, recipes};
use repo_policy::{read, repo_root, tracked};

const GATE: &str = ".github/workflows/on-pr-synced.yml";

fn workflow_text(path: &str) -> String {
    read(path)
}

fn workflow_paths() -> Vec<&'static str> {
    let paths: Vec<&'static str> = sources()
        .iter()
        .filter(|source| matches!(source.parsed, Executable::Workflow(_)))
        .map(|source| source.path.as_str())
        .collect();
    assert!(!paths.is_empty(), "no workflow to check; this would pass vacuously");
    paths
}

#[test]
fn ci_success_gathers_every_other_job_in_its_workflow() {
    let jobs: BTreeSet<String> =
        job_policies().into_iter().filter(|policy| policy.source == GATE).map(|policy| policy.name).collect();
    let gate = job_policies()
        .into_iter()
        .find(|policy| policy.source == GATE && policy.name == "ci-success")
        .expect("the gate workflow declares ci-success");

    let mut leaves = jobs;
    leaves.remove("ci-success");
    assert_eq!(leaves, gate.needs, "ci-success must gather every other job");

    let text = workflow_text(GATE);
    let allowed_block = text.split("allowed-skips: >-").nth(1).expect("ci-success declares allowed-skips");
    let allowed: BTreeSet<String> = allowed_block
        .split("\n\n")
        .next()
        .unwrap_or(allowed_block)
        .replace('\n', " ")
        .split(',')
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect();
    let mut expected = gate.needs.clone();
    expected.remove("changes");
    assert_eq!(expected, allowed, "every gathered job but `changes` may be skipped");
}

#[test]
fn the_classifier_declares_exactly_the_derived_classes_plus_the_pins() {
    let text = workflow_text(GATE);
    let block = text
        .split("\n    outputs:\n")
        .nth(1)
        .expect("the changes job declares outputs")
        .split("\n    steps:")
        .next()
        .expect("the outputs block ends at steps");
    let declared: BTreeSet<String> = block
        .lines()
        .filter_map(|line| line.strip_prefix("      "))
        .filter_map(|line| line.split(':').next())
        .filter(|name| !name.is_empty() && !name.starts_with(' '))
        .map(str::to_owned)
        .collect();

    let action = read(&format!(".github/actions/{MANIFEST_ACTION}/action.yml"));
    let outputs = action
        .split("\noutputs:\n")
        .nth(1)
        .expect("the pins action declares outputs")
        .split("\nruns:")
        .next()
        .expect("the outputs block ends at runs");
    let pins: BTreeSet<String> = outputs
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter(|line| line.ends_with(':') && !line.starts_with(' '))
        .map(|line| line.trim_end_matches(':').to_owned())
        .collect();
    assert!(!pins.is_empty(), "the pins action declares no named output");

    let derived: BTreeSet<String> = DERIVED_CLASSES.iter().map(|entry| entry.name.to_owned()).collect();
    let expected: BTreeSet<String> = derived.union(&pins).cloned().collect();
    assert_eq!(
        expected, declared,
        "a declared output nothing derives is dead; a derived class nothing declares is unreachable"
    );
}

#[test]
fn no_action_installs_a_rust_toolchain() {
    let actions: Vec<&str> = sources()
        .iter()
        .filter(|source| matches!(source.parsed, Executable::Action(_)))
        .map(|source| source.path.as_str())
        .collect();
    assert!(!actions.is_empty(), "no action to check; this would pass vacuously");
    for path in actions {
        assert!(
            !read(path).contains("dtolnay/rust-toolchain"),
            "{path} installs a toolchain, which no job owns and nothing can classify"
        );
    }
}

/// The channel a `toolchain:` line reads, as the manifest path it indexes.
fn selected_channel(value: &str) -> String {
    let indexed = format!("{MANIFEST_OUTPUT}).rust.");
    if let Some(start) = value.find(&indexed) {
        let rest = &value[start + indexed.len()..];
        let name: String =
            rest.chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_').collect();
        return format!("rust.{name}");
    }
    if value.contains("matrix.toolchain") {
        return "matrix".to_owned();
    }
    value.trim().to_owned()
}

#[test]
fn every_toolchain_step_selects_the_channel_its_own_work_reads() {
    let entries = lane_entry_recipes();
    let mut checked: BTreeMap<&str, usize> = BTreeMap::from([("extension lane", 0), ("public workspace", 0)]);

    for policy in job_policies() {
        let text = read(policy.source);
        let body = job_body(&text, &policy.name);
        let enters_lane = entries.iter().any(|recipe| {
            body.contains(&format!("just {recipe} ")) || body.trim_end().ends_with(&format!("just {recipe}"))
        });
        let (expected, where_) =
            if enters_lane { ("rust.lane", "extension lane") } else { ("rust.primary", "public workspace") };
        let allowed: BTreeSet<&str> = if enters_lane {
            // Miri is the exception the lane actually has; a matrix is not, and would install
            // the public workspace's MSRV into a lane job.
            BTreeSet::from([expected, "nightly"])
        } else {
            BTreeSet::from([expected, "nightly", "matrix"])
        };

        for step in body.split("      - uses: dtolnay/rust-toolchain@").skip(1) {
            let selected: BTreeSet<String> = step
                .lines()
                .take_while(|line| !line.trim_start().starts_with("- "))
                .filter_map(|line| line.trim().strip_prefix("toolchain:"))
                .map(selected_channel)
                .collect();
            assert_eq!(1, selected.len(), "{}: one toolchain per step", policy.name);
            let selected_refs: BTreeSet<&str> = selected.iter().map(String::as_str).collect();
            assert!(
                selected_refs.is_subset(&allowed),
                "{} selects {selected_refs:?} but works in the {where_}, which reads {expected}",
                policy.name
            );
            // Only a step reading the PINNED channel counts: `nightly` is allowed on both sides,
            // so counting it would leave a tally non-zero with no job compared to its own pin.
            if selected_refs == BTreeSet::from([expected]) {
                *checked.get_mut(where_).expect("both sides are tallied") += 1;
            }
        }
    }

    for (side, count) in checked {
        assert!(count > 0, "no {side} toolchain step checked; this would pass vacuously");
    }
}

/// The text written under one job id.
fn job_body(text: &str, job: &str) -> String {
    let jobs = text.split("\njobs:\n").nth(1).expect("the workflow has a jobs block");
    let start = jobs.find(&format!("\n  {job}:\n")).map_or_else(
        || jobs.starts_with(&format!("  {job}:\n")).then_some(0).expect("the job is declared"),
        |index| index + 1,
    );
    let rest = &jobs[start..];
    let mut end = rest.len();
    for (offset, _) in rest.match_indices("\n  ") {
        let after = &rest[offset + 3..];
        if after.starts_with(|c: char| c.is_ascii_lowercase())
            && after.split(':').next().is_some_and(|name| {
                !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            })
            && offset > 0
        {
            end = offset;
            break;
        }
    }
    rest[..end].to_owned()
}

#[test]
fn no_step_repeats_a_mapping_key() {
    for path in workflow_paths() {
        let text = read(path);
        let lines: Vec<&str> = text.lines().collect();
        let mut starts: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with("      - ") && line.len() > 8)
            .map(|(index, _)| index)
            .collect();
        starts.push(lines.len());
        for window in starts.windows(2) {
            let mut keys = Vec::new();
            for line in &lines[window[0]..window[1]] {
                if let Some(rest) = line.strip_prefix("        ")
                    && let Some(key) = rest.split(':').next()
                    && !key.is_empty()
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    && rest[key.len()..].starts_with(':')
                {
                    keys.push(key.to_owned());
                }
            }
            let unique: BTreeSet<&String> = keys.iter().collect();
            assert_eq!(
                keys.len(),
                unique.len(),
                "{path}: a step repeats a mapping key near line {}",
                window[0] + 1
            );
        }
    }
}

#[test]
fn the_docs_job_installs_from_the_package_lock() {
    let text = workflow_text(GATE);
    assert!(text.contains("npm ci --no-fund --no-audit"));
    assert!(!text.contains("npm install "));
}

#[test]
fn every_registry_token_job_targets_the_protected_environment() {
    for path in workflow_paths() {
        let text = read(path);
        if text.contains("SECRET_DEPLOY_CRATEIO") {
            assert!(
                text.contains("    environment: crates-io"),
                "{path} holds the token outside its environment"
            );
        }
    }
}

#[test]
fn publish_all_verifies_the_workspace_together() {
    let recipes = recipes(&repo_root());
    let body = recipes["publish-all"].body();
    assert!(body.contains("cargo publish --workspace --dry-run --allow-dirty"));
    assert!(!body.contains("cargo publish -p"));
}

#[test]
fn a_path_filtered_job_is_never_suppressed_by_a_skipped_dependency() {
    let policies: BTreeMap<String, _> = job_policies()
        .into_iter()
        .filter(|policy| policy.source == GATE)
        .map(|policy| (policy.name.clone(), policy))
        .collect();

    // The simulation can only reason about conditions it understands. A gate job whose `if:`
    // it cannot read would be simulated as unconditional, clearing a job it never examined.
    for policy in policies.values() {
        assert!(
            policy.condition.is_none() || policy.always || policy.output.is_some(),
            "{}: the cascade simulation cannot read the condition {:?}",
            policy.name,
            policy.condition
        );
    }

    let mut offenders = Vec::new();
    for path in tracked(&["."]) {
        let outputs = classify_paths([&path]).expect("every tracked path is classified");
        let direct: BTreeMap<&str, bool> = policies
            .iter()
            .map(|(name, policy)| {
                let selected = policy.output.as_ref().is_none_or(|output| outputs[output.as_str()]);
                (name.as_str(), selected)
            })
            .collect();

        let mut scheduled: BTreeMap<&str, bool> = BTreeMap::new();
        for name in policies.keys() {
            resolve(name.as_str(), &policies, &direct, &mut scheduled);
        }

        for (name, selected) in &direct {
            if *selected && !policies[*name].always && !scheduled[name] {
                offenders.push(format!("{path}: {name}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "jobs selected by their path condition but suppressed by a skipped dependency: {offenders:?}"
    );
}

fn resolve<'a>(
    job: &'a str,
    policies: &'a BTreeMap<String, repo_policy::actions::JobPolicy>,
    direct: &BTreeMap<&'a str, bool>,
    scheduled: &mut BTreeMap<&'a str, bool>,
) -> bool {
    if let Some(known) = scheduled.get(job) {
        return *known;
    }
    // Inserted before recursing so a cycle resolves rather than recursing forever; the workflow
    // graph is acyclic, and a cycle would be Actions' error to report, not this check's.
    scheduled.insert(job, false);
    let policy = &policies[job];
    let selected = direct[job]
        && (policy.always
            || policy.needs.iter().all(|need| {
                !policies.contains_key(need) || resolve(need.as_str(), policies, direct, scheduled)
            }));
    scheduled.insert(job, selected);
    selected
}
