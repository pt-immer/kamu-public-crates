//! One action, one pin, one label.

use std::collections::BTreeMap;

use repo_policy::actions::remote_uses;

/// Two references to the same action at different commits are two versions of it, and the
/// second is whichever a reader did not check.
#[test]
fn every_reference_to_an_action_names_the_same_commit() {
    let mut commits: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for reference in remote_uses() {
        commits
            .entry(reference.action)
            .or_default()
            .entry(reference.commit)
            .or_default()
            .push(reference.source);
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
    let mut labels: BTreeMap<String, BTreeMap<Option<String>, usize>> = BTreeMap::new();
    for reference in remote_uses() {
        *labels.entry(reference.action).or_default().entry(reference.label).or_default() += 1;
    }
    assert!(!labels.is_empty(), "no remote action reference to check");

    for (action, spellings) in labels {
        assert_eq!(spellings.len(), 1, "{action} carries several labels: {spellings:?}");
        assert!(
            spellings.keys().all(Option::is_some),
            "{action} is pinned without a label naming what the commit is",
        );
    }
}
