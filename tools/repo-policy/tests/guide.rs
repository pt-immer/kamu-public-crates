//! Every recipe a tracked document names must exist.
//!
//! A document naming a recipe is an edge, and renaming the recipe does not move it. Nothing
//! noticed until the reader ran the command: `just pg selftest-all` survived in three files after
//! the recipe went, and the failure it produced named a recipe rather than a document.
//!
//! Recipe names are mechanizable where most prose references are not, because `just --dump`
//! enumerates the whole domain. A check over paths named in prose would instead need standing
//! exemptions for URLs, submodule contents, external-crate paths and files Cargo generates -- a
//! maintained list deciding which references are real, which is the duplicate this repository is
//! removing rather than adding.

use std::collections::BTreeSet;

use regex_lite::Regex;
use repo_policy::justfile::recipes;
use repo_policy::{read, repo_root, tracked};

/// Durable history names recipes that are meant to be gone.
fn is_history(path: &str) -> bool {
    path.rsplit('/').next() == Some("CHANGELOG.md")
}

/// The parts of a document that write COMMANDS: fenced blocks and code spans.
///
/// In prose `just` is an adverb, and "the crate it just published" is not an invocation.
fn commands(markdown: &str) -> String {
    let fence = Regex::new(r"(?sm)^```.*?^```").expect("the fence pattern compiles");
    let span = Regex::new(r"`([^`\n]+)`").expect("the span pattern compiles");

    let mut code: Vec<String> = fence.find_iter(markdown).map(|found| found.as_str().to_owned()).collect();
    code.extend(span.captures_iter(markdown).map(|found| found[1].to_owned()));
    code.join("\n")
}

/// Every recipe invoked in `markdown`, as `(reaches_the_lane, name)`.
///
/// The separator is `[ \t]` rather than `\s`: a name must follow `just` on ONE line, or a match
/// stitches two unrelated code spans into an invocation neither document wrote.
fn invocations(markdown: &str) -> Vec<(bool, String)> {
    let call = Regex::new(r"\bjust[ \t]+(pg[ \t]+)?([a-z][a-z0-9-]*)").expect("the call pattern compiles");
    commands(markdown)
        .lines()
        .flat_map(|line| {
            call.captures_iter(line)
                .map(|found| (found.get(1).is_some(), found[2].to_owned()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `just pg <name>` addresses the lane. A bare `just <name>` may name either: a document inside
/// the lane describes lane recipes, and a document anywhere may describe a repository-level gate.
fn unresolved(markdown: &str, root: &BTreeSet<String>, lane: &BTreeSet<String>) -> Vec<String> {
    invocations(markdown)
        .into_iter()
        .filter(|(reaches_lane, name)| {
            if *reaches_lane { !lane.contains(name) } else { !root.contains(name) && !lane.contains(name) }
        })
        .map(|(reaches_lane, name)| format!("just {}{name}", if reaches_lane { "pg " } else { "" }))
        .collect()
}

fn known() -> (BTreeSet<String>, BTreeSet<String>) {
    let root = repo_root();
    let lane = root.join("extensions/money-pg");
    (recipes(&root).into_keys().collect(), recipes(&lane).into_keys().collect())
}

#[test]
fn every_recipe_a_document_names_exists() {
    let (root, lane) = known();
    assert!(!root.is_empty() && !lane.is_empty(), "no recipe to resolve against");

    let mut checked = 0_usize;
    let mut offenders = Vec::new();
    for path in tracked(&["*.md"]) {
        if is_history(&path) || path.starts_with("crates/iso3166/vendor/") {
            continue;
        }
        let markdown = read(&path);
        checked += invocations(&markdown).len();
        offenders
            .extend(unresolved(&markdown, &root, &lane).into_iter().map(|call| format!("{path}: {call}")));
    }

    assert!(checked > 50, "only {checked} invocations parsed; this would pass vacuously");
    assert!(offenders.is_empty(), "a document names a recipe that does not exist: {offenders:?}");
}

#[test]
fn a_document_naming_a_recipe_that_does_not_exist_is_reported() {
    let (root, lane) = known();

    assert_eq!(
        vec!["just selftest-all".to_owned()],
        unresolved("Run `just selftest-all` before pushing.", &root, &lane),
        "a retired recipe went unreported"
    );
    assert_eq!(
        vec!["just pg gate".to_owned()],
        unresolved("```sh\njust pg gate\n```", &root, &lane),
        "a root recipe addressed as a lane recipe went unreported"
    );
}

#[test]
fn prose_is_not_read_as_an_invocation() {
    let (root, lane) = known();

    // `just` as an adverb, and the backticked crate is not a recipe name.
    assert!(unresolved("the `kamu-money-core` it just published", &root, &lane).is_empty());
    // Two code spans, one line apart. Joining them would invent `just pg gate-all`, which no
    // document wrote and which would resolve against the wrong Justfile.
    assert!(
        unresolved("`just pg` is not the only way in, and `gate-all` composes it", &root, &lane).is_empty()
    );
}
