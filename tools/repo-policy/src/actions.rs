//! What GitHub Actions executes: the workflows, and the composite actions they call.
//!
//! Three readings, because the questions differ in kind.
//!
//! The typed model answers what a document *means*: which jobs exist, and what each declares
//! in `needs` whether written as a scalar or a list. Parsing into it also decides whether the
//! file is a workflow at all, so a malformed one fails here rather than being half-read by a
//! scan that finds nothing and reports nothing.
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

/// The action every pinned tool is installed through, as its repository.
///
/// The reference is not part of it because the action answers to two spellings and both have
/// to be recognised: `taiki-e/install-action@<ref>`, which takes a `tool:` list, and
/// `taiki-e/install-action/<tool>@<ref>`, which names one tool in its own path and states no
/// version at all.
pub const INSTALL_ACTION: &str = "taiki-e/install-action";

/// The name the pinned-version manifest is published and republished under. Every reader of a
/// manifest expression has to spell it, so it is stated here and held equal in the readers that
/// cannot import it.
pub const MANIFEST_OUTPUT: &str = "manifest";

/// The composite action that reads the manifest and publishes it once. Running it is what makes a
/// workflow a reader of the output above, so the set of readers inside Actions is derived from
/// this rather than maintained as a list.
pub const MANIFEST_ACTION: &str = "read-dev-tools";

/// The code half of a line, with any trailing comment removed.
///
/// A `#` opens a comment where a value is not already open: at the start of the line, or after
/// whitespace outside quotes. A line that ends inside a quote never opened one -- that was an
/// apostrophe in plain text -- and is read again with quoting ignored.
pub fn code_of(line: &str) -> &str {
    let scan = |respect_quotes: bool| {
        let mut quote = None;
        let mut start = None;
        let mut escaped = false;
        for (index, character) in line.char_indices() {
            // A backslash escapes the next character inside double quotes, and is literal inside
            // single ones. Reading `\"` as the close puts the rest of the string outside the
            // quote, where a `#` reads as a comment and takes executable content out of the scan.
            if escaped {
                escaped = false;
                continue;
            }
            if quote == Some('"') && character == '\\' {
                escaped = true;
                continue;
            }
            if let Some(open) = quote {
                if character == open {
                    quote = None;
                }
            } else if respect_quotes && (character == '\'' || character == '"') {
                quote = Some(character);
            } else if character == '#' && (index == 0 || line[..index].ends_with(char::is_whitespace)) {
                start = Some(index);
                break;
            }
        }
        (quote.is_none(), start)
    };
    let (balanced, start) = scan(true);
    let start = if balanced { start } else { scan(false).1 };
    start.map_or(line, |index| &line[..index])
}

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
    for (_, scope) in scopes_of(source) {
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
            // The label is a comment, so it is read from the line rather than the document --
            // and from EVERY line the clause is written on. Taking the first would leave one
            // label read and the rest bound by nothing, which is most of them.
            for line in source.text.lines().filter(|line| code_of(line).contains(clause)) {
                uses.push(Use {
                    source: source.path.clone(),
                    action: repository.slug().to_owned(),
                    pinned_to: repository.git_ref().to_owned(),
                    label: line.split_once('#').map(|(_, label)| label.trim().to_owned()),
                });
            }
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

/// Every step that uses the tool installer, as `(source, scope, uses clause, tool list)`.
///
/// The list is read from the parsed step rather than from the line, because it is accepted as
/// a plain scalar, as a block scalar spanning lines, and inside a flow mapping, and only the
/// document knows which of those a step wrote. It is `None` where the step states no `tool:`
/// input, which is the whole of what the sub-action spelling can say.
///
/// The scope is the job, so a failure among many steps in one file says which job to open.
pub fn install_action_steps() -> Vec<(&'static str, String, String, Option<String>)> {
    let mut steps = Vec::new();
    for source in sources() {
        for (scope, body) in scopes_of(source) {
            for step in steps_of(body) {
                let Some(clause) = child(step, "uses").and_then(Value::as_str) else {
                    continue;
                };
                let action = clause.split_once('@').map_or(clause, |(action, _)| action);
                // The input is only a tool list on the action that installs tools; another
                // action taking an input of the same name owes this nothing.
                if !action
                    .strip_prefix(INSTALL_ACTION)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                {
                    continue;
                }
                let requested = child(step, "with").and_then(|with| child(with, "tool")).map(|tool| {
                    tool.as_str()
                        .unwrap_or_else(|| {
                            panic!("{} job {scope} writes a tool list that is not text", source.path)
                        })
                        .to_owned()
                });
                steps.push((source.path.as_str(), scope.clone(), clause.to_owned(), requested));
            }
        }
    }
    steps
}

/// Every tool an install-action step requests, as `(source, scope, specification)`.
pub fn tool_requests() -> Vec<(&'static str, String, String)> {
    let mut requests = Vec::new();
    for (source, scope, _, requested) in install_action_steps() {
        let Some(requested) = requested else {
            continue;
        };
        // Both separators, because the block form writes one tool per line.
        for specification in requested.split([',', '\n']) {
            let specification = specification.trim();
            if !specification.is_empty() {
                requests.push((source, scope.clone(), specification.to_owned()));
            }
        }
    }
    requests
}

/// One output a composite action publishes.
#[derive(Debug)]
pub struct ActionOutput {
    /// The action's path.
    pub source: &'static str,
    /// The name a caller reads it under.
    pub name: String,
    /// The expression it resolves to.
    pub value: String,
    /// Every step this action declares an id for, with its `run` body where it has one. A step
    /// that runs another action has none, and an output read from it is beyond what this
    /// models -- which is a different answer from a step that is not there at all.
    pub steps: Vec<(String, Option<String>)>,
}

/// Every output a composite action publishes.
///
/// A workflow states its outputs under a job or under `on.workflow_call`, so a top-level
/// `outputs` key belongs to an action.
pub fn action_outputs() -> Vec<ActionOutput> {
    let mut outputs = Vec::new();
    for source in sources() {
        let Some(runs) = child(&source.tree, "runs") else {
            continue;
        };
        let Some(entries) = child(&source.tree, "outputs").and_then(Value::as_mapping) else {
            continue;
        };
        let steps: Vec<(String, Option<String>)> = steps_of(runs)
            .into_iter()
            .filter_map(|step| {
                let id = child(step, "id").and_then(Value::as_str)?;
                let body = child(step, "run").and_then(Value::as_str).map(str::to_owned);
                Some((id.to_owned(), body))
            })
            .collect();
        for (name, entry) in entries {
            let (Some(name), Some(value)) = (name.as_str(), child(entry, "value").and_then(Value::as_str))
            else {
                continue;
            };
            outputs.push(ActionOutput {
                source: source.path.as_str(),
                name: name.to_owned(),
                value: value.to_owned(),
                steps: steps.clone(),
            });
        }
    }
    outputs
}

/// What a `fromJSON` call is reading the manifest out of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestSource {
    /// `needs.<job>.outputs.manifest`, naming the job that republished it.
    Job(String),
    /// `steps.<id>.outputs.manifest`, naming the step that read it.
    Step(String),
}

/// The context a manifest argument names, or `None` when it names none.
///
/// Only two contexts can carry the manifest. Accepting the rest -- or accepting any expression
/// that merely ends in `outputs.manifest`, which `env.pins_outputs.manifest` does -- treats a
/// context nothing sets as a read, and every pin indexed out of it is the empty string.
pub fn manifest_source(argument: &str) -> Option<ManifestSource> {
    let suffix = format!(".outputs.{MANIFEST_OUTPUT}");
    let named = |rest: &str| -> Option<String> {
        let name = rest.strip_suffix(&suffix)?;
        let legal =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        legal.then(|| name.to_owned())
    };
    if let Some(rest) = argument.strip_prefix("needs.") {
        return named(rest).map(ManifestSource::Job);
    }
    argument.strip_prefix("steps.").and_then(named).map(ManifestSource::Step)
}

/// Every path an expression indexes out of the published manifest, as its segments.
///
/// Both spellings are read, because a key carrying a hyphen has to be indexed rather than
/// dereferenced: `.rust.primary` and `.cargo_tools['cargo-nextest'].version` are the same
/// kind of claim about the same document.
pub fn manifest_paths(expression: &str) -> Vec<Vec<String>> {
    // A read is the whole call, not the name of its argument: `steps.pins.outputs.manifest`
    // republished by a job reads nothing out of the document. The argument is taken up to the
    // call's own closing parenthesis, past whatever spacing the author left, so a differently
    // spaced call is still a read -- and a path never parsed is a path never checked.
    //
    // A newline ends the argument. Scanning whole-file text, an `outputs.manifest` ending one
    // line would otherwise reach the `)` opening the next and read that expression's path.
    const CALL: &str = "fromJSON(";
    let mut paths = Vec::new();
    let mut rest = expression;
    while let Some(start) = rest.find(CALL) {
        rest = &rest[start + CALL.len()..];
        let Some(end) = rest.find(')') else { break };
        let (argument, after) = rest.split_at(end);
        if argument.contains('\n') || manifest_source(argument.trim_matches([' ', '\t'])).is_none() {
            continue;
        }
        rest = &after[1..];
        let mut segments = Vec::new();
        loop {
            match rest.chars().next() {
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
