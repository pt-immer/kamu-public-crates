//! Every documented recipe must resolve in the repository Justfile.

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

/// Recipe names must follow `just` on the same line inside a code span or fence.
fn invocations(markdown: &str) -> Vec<String> {
    let call = Regex::new(r"\bjust[ \t]+([a-z][a-z0-9-]*)").unwrap();
    commands(markdown)
        .lines()
        .flat_map(|line| call.captures_iter(line).map(|found| found[1].to_owned()).collect::<Vec<_>>())
        .collect()
}

fn unresolved(markdown: &str, known: &BTreeSet<String>) -> Vec<String> {
    invocations(markdown).into_iter().filter(|name| !known.contains(name)).collect()
}

#[test]
fn every_recipe_a_document_names_exists() {
    let known = recipes(&repo_root()).into_keys().collect();
    let mut checked = 0;
    let mut offenders = Vec::new();
    for path in tracked(&["*.md"]) {
        if is_history(&path) || path.starts_with("crates/iso3166/vendor/") {
            continue;
        }
        let markdown = read(&path);
        checked += invocations(&markdown).len();
        offenders
            .extend(unresolved(&markdown, &known).into_iter().map(|call| format!("{path}: just {call}")));
    }
    assert!(checked > 20, "only {checked} invocations parsed");
    assert!(offenders.is_empty(), "unknown documented recipes: {offenders:?}");
}

#[test]
fn missing_recipes_are_reported_without_reading_prose_as_commands() {
    let known = recipes(&repo_root()).into_keys().collect();
    assert_eq!(vec!["selftest-all"], unresolved("Run `just selftest-all`.", &known));
    assert_eq!(vec!["pg"], unresolved("```sh\njust pg gate\n```", &known));
    assert!(unresolved("the `kamu-money-core` it just published", &known).is_empty());
    assert!(unresolved("`just` followed by `gate`", &known).is_empty());
}

/// Every cargo tool a job's recipes invoke must be a tool that job installs.
///
/// A recipe's body decides which tools it needs, and nothing bound that to the jobs that run it.
/// Measured: `doc-gate-selftest` ran a shell script, became a `cargo nextest run`, and the job
/// reaching it installed only `just`. No local gate can catch that -- the recipe passes on a
/// machine that already has the runner -- so it failed in CI after review.
///
/// `cargo_tools` only. Node tools arrive through `npm ci` and system tools come with the runner
/// image; the manifest's own sections draw that line, so it is not a maintained exemption list.
mod provisioning {
    use std::collections::{BTreeMap, BTreeSet};

    use regex_lite::Regex;
    use repo_policy::dev_env::{load_manifest, tools};
    use repo_policy::justfile::{Recipe, recipes};
    use repo_policy::{read, repo_root};

    const WORKFLOW: &str = ".github/workflows/on-pr-synced.yml";

    /// How a recipe writes this tool. A `cargo-x` crate installs a `cargo-x` binary and is
    /// invoked as `cargo x` -- cargo's subcommand convention, and the reason matching the binary
    /// name alone found nothing for the defect above.
    fn spellings(binary: &str) -> Vec<String> {
        let mut forms = vec![binary.to_owned()];
        if let Some(subcommand) = binary.strip_prefix("cargo-") {
            forms.push(format!("cargo {subcommand}"));
        }
        forms
    }

    /// A recipe and everything it composes: a job running an aggregate needs every tool the
    /// aggregate's dependencies invoke, not only the ones its own body names.
    fn closure(recipes: &BTreeMap<String, Recipe>, name: &str, reached: &mut BTreeSet<String>) {
        if !reached.insert(name.to_owned()) {
            return;
        }
        if let Some(recipe) = recipes.get(name) {
            for dependency in &recipe.dependencies {
                closure(recipes, &dependency.recipe, reached);
            }
        }
    }

    fn required(
        recipes: &BTreeMap<String, Recipe>,
        entry: &str,
        pinned: &[(String, Vec<String>)],
    ) -> BTreeSet<String> {
        let mut reached = BTreeSet::new();
        closure(recipes, entry, &mut reached);
        let text: String = reached
            .iter()
            .filter_map(|name| recipes.get(name))
            .map(Recipe::body)
            .collect::<Vec<_>>()
            .join("\n");
        pinned
            .iter()
            .filter(|(_, forms)| {
                forms.iter().any(|form| {
                    Regex::new(&format!(r"\b{}\b", regex_lite::escape(form)))
                        .expect("the tool pattern compiles")
                        .is_match(&text)
                })
            })
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Each job block of the gate workflow, by name.
    fn jobs(workflow: &str) -> BTreeMap<String, String> {
        let body = workflow.split("\njobs:\n").nth(1).expect("the workflow declares jobs");
        let starts: Vec<usize> = Regex::new(r"(?m)^  [a-z][a-z0-9-]*:$")
            .expect("the job pattern compiles")
            .find_iter(body)
            .map(|found| found.start())
            .collect();
        assert!(!starts.is_empty(), "no job parsed; this would pass vacuously");

        let mut parsed = BTreeMap::new();
        for (index, start) in starts.iter().enumerate() {
            let end = starts.get(index + 1).copied().unwrap_or(body.len());
            let block = &body[*start..end];
            let name = block.trim_start().split(':').next().expect("a job has a name").to_owned();
            parsed.insert(name, block.to_owned());
        }
        parsed
    }

    #[test]
    fn every_job_installs_the_cargo_tools_its_recipes_invoke() {
        let root = repo_root();
        let manifest = load_manifest(&root);
        let pinned: Vec<(String, Vec<String>)> = tools(&manifest, "cargo_tools")
            .into_iter()
            .map(|tool| (tool.name, spellings(&tool.binary)))
            .collect();
        assert!(!pinned.is_empty(), "the manifest pins no cargo tool");

        let root_recipes = recipes(&root);
        let workflow = read(WORKFLOW);
        let installs = Regex::new(r"cargo_tools\['([a-z0-9-]+)'\]").expect("the install pattern compiles");
        let step = Regex::new(r"run: just ([a-z][a-z0-9-]*)").expect("the step pattern compiles");

        let mut checked = 0_usize;
        let mut gaps = Vec::new();
        for (job, block) in jobs(&workflow) {
            let installed: BTreeSet<String> =
                installs.captures_iter(&block).map(|found| found[1].to_owned()).collect();
            for found in step.captures_iter(&block) {
                let entry = &found[1];
                let recipes = &root_recipes;
                checked += 1;
                for tool in required(recipes, entry, &pinned).difference(&installed) {
                    gaps.push(format!(
                        "{job}: `just {entry}` invokes {tool}, which the job does not install"
                    ));
                }
            }
        }

        assert!(checked > 20, "only {checked} steps parsed; this would pass vacuously");
        assert!(gaps.is_empty(), "a job runs a recipe whose tools it does not provision: {gaps:?}");
    }
}
