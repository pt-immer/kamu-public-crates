//! What GitHub Actions executes: the workflows, and the composite actions they call.

use crate::{read, tracked};

/// Both extensions are accepted for workflows and for composite actions, so reading one
/// spelling reports green over a file that runs and was never opened.
const PATTERNS: [&str; 4] = [
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
    ".github/actions/*/action.yml",
    ".github/actions/*/action.yaml",
];

/// Every file Actions executes, as `(repository-relative path, contents)`, in stable order.
pub fn sources() -> Vec<(String, String)> {
    let mut sources: Vec<(String, String)> = tracked(&PATTERNS)
        .into_iter()
        .map(|relative| {
            let text = read(&relative);
            (relative, text)
        })
        .collect();
    sources.sort();
    sources
}

/// One `uses:` reference: the action, the commit it is pinned to, and the label beside it.
#[derive(Debug, PartialEq, Eq)]
pub struct Use {
    pub source: String,
    pub action: String,
    pub commit: String,
    pub label: Option<String>,
}

/// Every remote action reference. A local one, written `./path`, is repository content that
/// the commit already versions and carries no pin of its own.
pub fn remote_uses() -> Vec<Use> {
    let mut uses = Vec::new();
    for (source, text) in sources() {
        for line in text.lines() {
            // A step writes `- uses:`; a job's `uses:` for a reusable workflow has no dash.
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
            let Some((action, commit)) = reference.split_once('@') else {
                continue;
            };
            uses.push(Use {
                source: source.clone(),
                action: action.to_owned(),
                commit: commit.to_owned(),
                label,
            });
        }
    }
    uses
}
