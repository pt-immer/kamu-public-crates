//! What GitHub Actions executes: the workflows, and the composite actions they call.

use crate::{read, tracked};

/// Both extensions are accepted everywhere Actions reads YAML. The patterns are built rather
/// than written so a spelling cannot be omitted by typing one of them wrong.
const DIRECTORIES: [&str; 2] = [".github/workflows/*", ".github/actions/*/action"];
const EXTENSIONS: [&str; 2] = ["yml", "yaml"];

fn patterns() -> Vec<String> {
    DIRECTORIES
        .iter()
        .flat_map(|directory| EXTENSIONS.iter().map(move |extension| format!("{directory}.{extension}")))
        .collect()
}

/// Every file Actions executes, as `(repository-relative path, contents)`, in stable order.
pub fn sources() -> Vec<(String, String)> {
    let owned = patterns();
    let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
    let mut sources: Vec<(String, String)> = tracked(&borrowed)
        .into_iter()
        .map(|relative| {
            let text = read(&relative);
            (relative, text)
        })
        .collect();
    sources.sort();
    sources
}

/// The composite actions this repository defines.
pub fn composite_actions() -> Vec<(String, String)> {
    sources().into_iter().filter(|(path, _)| path.starts_with(".github/actions/")).collect()
}

/// One `uses:` reference: the action, what it is pinned to, and the label beside it.
#[derive(Debug, PartialEq, Eq)]
pub struct Use {
    pub source: String,
    pub action: String,
    pub pinned_to: String,
    pub label: Option<String>,
}

/// Every remote action reference. A local one, written `./path`, is repository content that
/// this commit already versions and carries no pin of its own.
pub fn remote_uses() -> Vec<Use> {
    let mut uses = Vec::new();
    for (source, text) in sources() {
        for line in text.lines() {
            // A step writes `- uses:`; a job calling a reusable workflow has no dash.
            let trimmed = line.trim_start();
            let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            let Some(reference) = trimmed.strip_prefix("uses:").map(str::trim) else {
                continue;
            };
            if reference.starts_with("./") {
                continue;
            }
            let (reference, label) = match reference.split_once('#') {
                Some((reference, label)) => (reference.trim(), Some(label.trim().to_owned())),
                None => (reference, None),
            };
            let (action, pinned_to) = match reference.split_once('@') {
                Some((action, pinned_to)) => (action.to_owned(), pinned_to.to_owned()),
                None => (reference.to_owned(), String::new()),
            };
            uses.push(Use { source: source.clone(), action, pinned_to, label });
        }
    }
    uses
}

/// A `key: value` mapping entry at an exact indentation, which is how a YAML block distinguishes
/// a key from the continuation of a description above it.
pub fn entries_at(text: &str, indent: usize) -> Vec<(String, String)> {
    let prefix = " ".repeat(indent);
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix(&prefix)?;
            if rest.starts_with(' ') || rest.starts_with('-') {
                return None;
            }
            let (key, value) = rest.split_once(':')?;
            if key.is_empty() || key.contains(' ') {
                return None;
            }
            Some((key.to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Every `id:` a file declares for a step.
pub fn step_ids(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            trimmed.strip_prefix("id:").map(|value| value.trim().to_owned())
        })
        .collect()
}

/// Every `steps.<id>.outputs.<name>` a file reads, as `(id, name)`.
pub fn step_output_references(text: &str) -> Vec<(String, String)> {
    let mut references = Vec::new();
    let mut rest = text;
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
