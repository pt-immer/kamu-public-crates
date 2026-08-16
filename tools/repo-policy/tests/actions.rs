//! What Actions executes, and what it is allowed to execute.

use std::collections::{BTreeMap, BTreeSet};

use repo_policy::actions::{needs_output_references, remote_uses, step_output_references, step_scopes};

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
    let mut checked = 0_usize;
    for scope in step_scopes() {
        for expression in &scope.expressions {
            for (id, name) in step_output_references(expression) {
                assert!(
                    scope.declared.contains(&id),
                    "{} reads steps.{id}.outputs.{name} in {}, which declares no step {id}",
                    scope.source,
                    scope.name,
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no step output was read; this would pass vacuously");
}

/// `needs.<job>` resolves for a job this one depends on, and to the empty string for any other
/// -- Actions does not refuse it. A job reading a pin it did not wait for installs nothing and
/// compiles against whatever the runner already had, which is the failure a version literal
/// could not have. Reaching the pins through `needs` is what made this possible.
#[test]
fn every_needs_output_read_names_a_job_the_reader_depends_on() {
    let mut checked = 0_usize;
    for scope in step_scopes() {
        let Some(needs) = &scope.needs else {
            continue;
        };
        for expression in &scope.expressions {
            for (job, name) in needs_output_references(expression) {
                assert!(
                    needs.contains(&job),
                    "{} reads needs.{job}.outputs.{name} in {}, which does not declare {job} \
                     in needs; the expression resolves to the empty string",
                    scope.source,
                    scope.name,
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no needs output was read; this would pass vacuously");
}
