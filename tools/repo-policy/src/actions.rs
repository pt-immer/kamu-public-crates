//! What GitHub Actions executes: the workflows, and the composite actions they call.
//!
//! Three readings, because the questions differ in kind.
//!
//! The typed model answers what a document *means*: which jobs exist, what each declares in
//! `needs` whether written as a scalar or a list, what an action publishes under which name.
//! Parsing into it also decides whether the file is a workflow at all, so a malformed one
//! fails here rather than being half-read by a scan that finds nothing and reports nothing.
//!
//! The generic tree answers "every string anywhere under this job", and "the direct keys of
//! each step". Neither can be asked of the typed model without naming the fields involved,
//! and a list of fields goes stale the moment Actions grows one.
//!
//! Raw text answers the two questions that are about the bytes rather than the document: a
//! version literal, and the comment beside a pin, which every parser discards by design.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use github_actions_models::action::Action;
use github_actions_models::common::Uses;
use github_actions_models::workflow::{Job, Workflow};
use yaml_serde::Value;

use crate::{read, tracked};

/// The action every pinned tool is installed through.
pub const INSTALL_ACTION: &str = "taiki-e/install-action@";

/// A file GitHub Actions executes, parsed as what it is.
#[derive(Debug)]
pub enum Executable {
    /// A workflow.
    Workflow(Box<Workflow>),
    /// An action, composite or otherwise.
    Action(Box<Action>),
}

/// One executed file: where it is, what it says, and both readings of it.
#[derive(Debug)]
pub struct Source {
    /// Repository-relative path.
    pub path: String,
    /// The bytes, for the claims that are about the bytes.
    pub text: String,
    /// The untyped document, for harvesting that must not name a field.
    pub tree: Value,
    /// The typed document.
    pub parsed: Executable,
}

/// GitHub reads workflows from `.github/workflows` itself and does not recurse, so a file one
/// directory deeper is not a workflow however much it looks like one.
fn is_workflow(path: &str) -> bool {
    path.strip_prefix(".github/workflows/").is_some_and(|rest| !rest.contains('/'))
}

/// An action is named by its own file, so `uses: ./tools/ci-action` resolves at any depth.
/// Keying on a directory would find only the actions that happen to live where we put them.
fn is_action(path: &str) -> bool {
    matches!(path.rsplit('/').next(), Some("action.yml" | "action.yaml"))
}

fn parse<T: serde::de::DeserializeOwned>(path: &str, text: &str) -> T {
    yaml_serde::from_str(text)
        .unwrap_or_else(|error| panic!("{path} is not something Actions can run: {error}"))
}

/// Every file Actions executes, in stable order.
///
/// Read once: every check below walks the same set, and the listing costs a subprocess.
pub fn sources() -> &'static [Source] {
    static SOURCES: OnceLock<Vec<Source>> = OnceLock::new();
    SOURCES.get_or_init(|| {
        let mut sources: Vec<Source> = tracked(&["*.yml", "*.yaml"])
            .into_iter()
            .filter(|path| is_workflow(path) || is_action(path))
            .map(|path| {
                let text = read(&path);
                let tree = parse(&path, &text);
                let parsed = if is_workflow(&path) {
                    Executable::Workflow(Box::new(parse(&path, &text)))
                } else {
                    Executable::Action(Box::new(parse(&path, &text)))
                };
                Source { path, text, tree, parsed }
            })
            .collect();
        sources.sort_by(|left, right| left.path.cmp(&right.path));
        assert!(
            sources.iter().any(|source| matches!(source.parsed, Executable::Workflow(_))),
            "no workflow was found; every check over them would pass vacuously",
        );
        assert!(
            sources.iter().any(|source| matches!(source.parsed, Executable::Action(_))),
            "no action was found; every check over them would pass vacuously",
        );
        sources
    })
}

/// Every action this repository defines, as `(path, model)`.
pub fn actions() -> Vec<(&'static str, &'static Action)> {
    sources()
        .iter()
        .filter_map(|source| match &source.parsed {
            Executable::Action(action) => Some((source.path.as_str(), &**action)),
            Executable::Workflow(_) => None,
        })
        .collect()
}

/// What each job republishes under `outputs:`, as `(source, job, name, value)`. Other jobs
/// reach a pin through these, so one renamed on the way through is read under a name that no
/// longer describes it.
pub fn job_outputs() -> Vec<(&'static str, String, String, String)> {
    let mut outputs = Vec::new();
    for source in sources() {
        let Some(jobs) = child(&source.tree, "jobs").and_then(Value::as_mapping) else {
            continue;
        };
        for (job, body) in jobs {
            let Some(job) = job.as_str() else { continue };
            let Some(entries) = child(body, "outputs").and_then(Value::as_mapping) else {
                continue;
            };
            for (name, value) in entries {
                if let (Some(name), Some(value)) = (name.as_str(), value.as_str()) {
                    let source = source.path.as_str();
                    outputs.push((source, job.to_owned(), name.to_owned(), value.to_owned()));
                }
            }
        }
    }
    outputs
}

/// The value under `key`, when this value is a mapping that has one.
fn child<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_mapping()?.iter().find_map(|(name, value)| (name.as_str()? == key).then_some(value))
}

/// Every string anywhere beneath a value. An Actions expression can be written wherever a
/// string can, so this is what "every expression in this scope" has to mean.
fn strings<'a>(value: &'a Value, into: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => into.push(text),
        Value::Sequence(items) => items.iter().for_each(|item| strings(item, into)),
        Value::Mapping(entries) => entries.iter().for_each(|(key, value)| {
            strings(key, into);
            strings(value, into);
        }),
        _ => {}
    }
}

/// The step mappings under a scope, as written.
fn steps_of(scope: &Value) -> Vec<&Value> {
    child(scope, "steps").and_then(Value::as_sequence).map(|steps| steps.iter().collect()).unwrap_or_default()
}

/// A scope in which `steps.<id>` resolves: one job of a workflow, or a composite action, whose
/// steps share a single scope. An id declared in a sibling job does not resolve here.
#[derive(Debug)]
pub struct Scope {
    /// The file this scope is written in.
    pub source: &'static str,
    /// The job's key, or the action's path.
    pub name: String,
    /// The `id` each step declares. Read as a direct key of the step, so an action input that
    /// happens to be called `id` is not mistaken for one.
    pub declared: BTreeSet<String>,
    /// What this job declares in `needs`, or `None` for a composite action, which has no such
    /// notion. Absent and empty are different answers.
    pub needs: Option<BTreeSet<String>>,
    /// Every string written anywhere in this scope.
    pub expressions: Vec<&'static str>,
}

fn declared_ids(scope: &Value) -> BTreeSet<String> {
    steps_of(scope).into_iter().filter_map(|step| Some(child(step, "id")?.as_str()?.to_owned())).collect()
}

/// Every scope in which a `steps.<id>` reference resolves.
pub fn step_scopes() -> Vec<Scope> {
    let mut scopes = Vec::new();
    for source in sources() {
        match &source.parsed {
            Executable::Workflow(workflow) => {
                let jobs = child(&source.tree, "jobs");
                for (name, job) in &workflow.jobs {
                    let needs = match job {
                        Job::NormalJob(job) => job.needs.iter().cloned().collect(),
                        Job::ReusableWorkflowCallJob(job) => job.needs.iter().cloned().collect(),
                    };
                    // The typed model names the job; the tree carries what is written under it.
                    let Some(tree) = jobs.and_then(|jobs| child(jobs, name)) else {
                        continue;
                    };
                    let mut expressions = Vec::new();
                    strings(tree, &mut expressions);
                    scopes.push(Scope {
                        source: source.path.as_str(),
                        name: name.clone(),
                        declared: declared_ids(tree),
                        needs: Some(needs),
                        expressions,
                    });
                }
            }
            Executable::Action(_) => {
                let Some(runs) = child(&source.tree, "runs") else {
                    continue;
                };
                let mut expressions = Vec::new();
                strings(&source.tree, &mut expressions);
                scopes.push(Scope {
                    source: source.path.as_str(),
                    name: source.path.clone(),
                    declared: declared_ids(runs),
                    needs: None,
                    expressions,
                });
            }
        }
    }
    assert!(!scopes.is_empty(), "no scope was found; every check over them would pass vacuously");
    scopes
}

/// One `uses:` reference: the action, what it is pinned to, and the label beside it.
#[derive(Debug, PartialEq, Eq)]
pub struct Use {
    /// The file the reference is written in.
    pub source: String,
    /// `owner/repo`, with any subpath.
    pub action: String,
    /// The git ref the reference names.
    pub pinned_to: String,
    /// The comment beside it, which names what the commit is. Parsing discards comments, so
    /// this is read from the line the reference is written on.
    pub label: Option<String>,
}

/// Every `uses:` clause written anywhere: a step's, and a job's call to a reusable workflow.
///
/// Harvested from the tree rather than from the typed step variants, because Actions keeps
/// adding step forms and a match over the ones that exist today is a list that goes stale.
fn uses_clauses(source: &Source) -> Vec<&str> {
    let mut clauses = Vec::new();
    let mut scopes: Vec<&Value> = Vec::new();
    if let Some(jobs) = child(&source.tree, "jobs").and_then(Value::as_mapping) {
        scopes.extend(jobs.iter().map(|(_, job)| job));
    }
    if let Some(runs) = child(&source.tree, "runs") {
        scopes.push(runs);
    }
    for scope in scopes {
        // A job calling a reusable workflow carries `uses:` itself rather than in a step.
        if let Some(clause) = child(scope, "uses").and_then(Value::as_str) {
            clauses.push(clause);
        }
        for step in steps_of(scope) {
            if let Some(clause) = child(step, "uses").and_then(Value::as_str) {
                clauses.push(clause);
            }
        }
    }
    clauses
}

/// Every remote action reference. A local one, written `./path`, is repository content that
/// this commit already versions and carries no pin of its own.
pub fn remote_uses() -> Vec<Use> {
    let mut uses = Vec::new();
    for source in sources() {
        for clause in uses_clauses(source) {
            let parsed = Uses::parse(clause)
                .unwrap_or_else(|error| panic!("{} writes `uses: {clause}`: {error}", source.path));
            let Uses::Repository(repository) = parsed else {
                continue;
            };
            // The label is a comment, so it is read from the line rather than the document.
            let label = source
                .text
                .lines()
                .find(|line| line.contains(clause))
                .and_then(|line| line.split_once('#'))
                .map(|(_, label)| label.trim().to_owned());
            uses.push(Use {
                source: source.path.clone(),
                action: repository.slug().to_owned(),
                pinned_to: repository.git_ref().to_owned(),
                label,
            });
        }
    }
    uses
}

/// Every scope whose steps run together: each job of a workflow, or an action's `runs`.
fn scopes_of(source: &Source) -> Vec<(String, &Value)> {
    let mut scopes = Vec::new();
    if let Some(jobs) = child(&source.tree, "jobs").and_then(Value::as_mapping) {
        for (name, job) in jobs {
            if let Some(name) = name.as_str() {
                scopes.push((name.to_owned(), job));
            }
        }
    }
    if let Some(runs) = child(&source.tree, "runs") {
        scopes.push((source.path.clone(), runs));
    }
    scopes
}

/// What each step uses, as `(source, scope, step id, uses clause)`. A step with no id cannot
/// be referenced by one, and is not listed.
pub fn step_uses() -> Vec<(&'static str, String, String, String)> {
    let mut used = Vec::new();
    for source in sources() {
        for (scope, body) in scopes_of(source) {
            for step in steps_of(body) {
                let (Some(id), Some(clause)) =
                    (child(step, "id").and_then(Value::as_str), child(step, "uses").and_then(Value::as_str))
                else {
                    continue;
                };
                used.push((source.path.as_str(), scope.clone(), id.to_owned(), clause.to_owned()));
            }
        }
    }
    used
}

/// Every tool an install-action step requests, as `(source, scope, specification)`.
///
/// Read from the parsed step rather than from the line: the tool list is accepted as a plain
/// scalar, as a block scalar spanning lines, and inside a flow mapping, and only the document
/// knows which of those a step wrote. The scope is the job, so a failure among many requests
/// in one file says which job to open.
pub fn tool_requests() -> Vec<(&'static str, String, String)> {
    let mut requests = Vec::new();
    for source in sources() {
        let mut scopes: Vec<(String, &Value)> = Vec::new();
        if let Some(jobs) = child(&source.tree, "jobs").and_then(Value::as_mapping) {
            for (name, job) in jobs {
                if let Some(name) = name.as_str() {
                    scopes.push((name.to_owned(), job));
                }
            }
        }
        if let Some(runs) = child(&source.tree, "runs") {
            scopes.push((source.path.clone(), runs));
        }
        for (scope, body) in scopes {
            for step in steps_of(body) {
                // The input is only a tool list on the action that installs tools; another
                // action taking an input of the same name owes this nothing.
                let installs = child(step, "uses")
                    .and_then(Value::as_str)
                    .is_some_and(|clause| clause.starts_with(INSTALL_ACTION));
                let Some(requested) = child(step, "with").and_then(|with| child(with, "tool")) else {
                    continue;
                };
                if !installs {
                    continue;
                }
                let requested = requested.as_str().unwrap_or_else(|| {
                    panic!("{} job {scope} writes a tool list that is not text", source.path)
                });
                // Both separators, because the block form writes one tool per line.
                for specification in requested.split([',', '\n']) {
                    let specification = specification.trim();
                    if !specification.is_empty() {
                        requests.push((source.path.as_str(), scope.clone(), specification.to_owned()));
                    }
                }
            }
        }
    }
    requests
}

/// Every path an expression indexes out of the published manifest, as its segments.
///
/// Both spellings are read, because a key carrying a hyphen has to be indexed rather than
/// dereferenced: `.rust.primary` and `.cargo_tools['cargo-nextest'].version` are the same
/// kind of claim about the same document.
pub fn manifest_paths(expression: &str) -> Vec<Vec<String>> {
    const ANCHOR: &str = "outputs.manifest)";
    let mut paths = Vec::new();
    let mut rest = expression;
    while let Some(start) = rest.find(ANCHOR) {
        rest = &rest[start + ANCHOR.len()..];
        let mut segments = Vec::new();
        loop {
            let mut characters = rest.chars();
            match characters.next() {
                Some('.') => {
                    let tail = &rest[1..];
                    let width =
                        tail.find(|c: char| !c.is_ascii_alphanumeric() && c != '_').unwrap_or(tail.len());
                    if width == 0 {
                        break;
                    }
                    segments.push(tail[..width].to_owned());
                    rest = &tail[width..];
                }
                Some('[') => {
                    let tail = &rest[1..];
                    let quote = match tail.chars().next() {
                        Some(quote @ ('\'' | '"')) => quote,
                        _ => break,
                    };
                    let body = &tail[1..];
                    let Some(width) = body.find(quote) else { break };
                    segments.push(body[..width].to_owned());
                    let after = &body[width + 1..];
                    match after.strip_prefix(']') {
                        Some(after) => rest = after,
                        None => break,
                    }
                }
                _ => break,
            }
        }
        if !segments.is_empty() {
            paths.push(segments);
        }
    }
    paths
}

/// Every `steps.<id>.outputs.<name>` an expression reads, as `(id, name)`.
pub fn step_output_references(expression: &str) -> Vec<(String, String)> {
    let mut references = Vec::new();
    let mut rest = expression;
    while let Some(start) = rest.find("steps.") {
        rest = &rest[start + "steps.".len()..];
        let Some((id, tail)) = rest.split_once(".outputs.") else {
            continue;
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        let name: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            references.push((id.to_owned(), name));
        }
    }
    references
}

/// Every `needs.<job>.outputs.<name>` an expression reads, as `(job, name)`.
pub fn needs_output_references(expression: &str) -> Vec<(String, String)> {
    let mut references = Vec::new();
    let mut rest = expression;
    while let Some(start) = rest.find("needs.") {
        rest = &rest[start + "needs.".len()..];
        let Some((job, tail)) = rest.split_once(".outputs.") else {
            continue;
        };
        if job.is_empty() || !job.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        let name: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            references.push((job.to_owned(), name));
        }
    }
    references
}
