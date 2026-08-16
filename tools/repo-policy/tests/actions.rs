//! What Actions executes, and what it is allowed to execute.

use std::collections::{BTreeMap, BTreeSet};

use repo_policy::actions::{remote_uses, sources, step_ids, step_output_references, step_scopes};

fn is_commit(pinned_to: &str) -> bool {
    pinned_to.len() == 40 && pinned_to.chars().all(|c| c.is_ascii_hexdigit())
}

/// A tag or branch moves under the pin. This covers the composite actions as well as the
/// workflows, because the job that gates every other job runs one of them.
#[test]
fn every_remote_action_is_pinned_to_a_commit() {
    let uses = remote_uses();
    assert!(!uses.is_empty(), "no remote action reference to check");
    for reference in uses {
        assert!(
            is_commit(&reference.pinned_to),
            "{} pins {} to {}, which is not a 40-character commit id",
            reference.source,
            reference.action,
            reference.pinned_to,
        );
    }
}

/// Two references to the same action at different commits are two versions of it, and the
/// second is whichever a reader did not check.
#[test]
fn every_reference_to_an_action_names_the_same_commit() {
    let mut commits: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for reference in remote_uses() {
        commits.entry(reference.action).or_default().insert(reference.pinned_to);
    }
    assert!(!commits.is_empty(), "no remote action reference to check");
    for (action, pinned) in commits {
        assert_eq!(pinned.len(), 1, "{action} is pinned to several commits: {pinned:?}");
    }
}

/// The label beside a pin is the only readable statement of which version a commit is. Two
/// spellings for one commit means at least one of them is a guess.
#[test]
fn every_reference_to_an_action_carries_the_same_label() {
    let mut labels: BTreeMap<String, BTreeSet<Option<String>>> = BTreeMap::new();
    for reference in remote_uses() {
        labels.entry(reference.action).or_default().insert(reference.label);
    }
    assert!(!labels.is_empty(), "no remote action reference to check");
    for (action, spellings) in labels {
        assert_eq!(spellings.len(), 1, "{action} carries several labels: {spellings:?}");
        assert!(
            spellings.iter().all(Option::is_some),
            "{action} is pinned without a label naming what the commit is",
        );
    }
}

/// Actions resolves `steps.<unknown>.outputs.<name>` to the empty string rather than failing,
/// so a renamed step id hands its consumer an empty value and the run continues.
#[test]
fn every_step_output_read_names_a_step_that_exists() {
    // Per job, not per file: `steps` is job-scoped, so an id declared in a sibling job does
    // not resolve here, and collecting ids file-wide would accept exactly that.
    let mut checked = 0_usize;
    for (source, text) in sources() {
        for scope in step_scopes(text) {
            let declared: BTreeSet<String> = step_ids(scope).into_iter().collect();
            for (id, name) in step_output_references(scope) {
                assert!(
                    declared.contains(&id),
                    "{source} reads steps.{id}.outputs.{name} in a job that declares no step {id}",
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no step output was read; this would pass vacuously");
}
