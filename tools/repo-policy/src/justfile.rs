//! The Justfile, read through `just --dump` rather than parsed again.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub dependencies: Vec<Dependency>,
    /// Each line as its fragments; an interpolation is a fragment of its own.
    body: Vec<Vec<serde_json::Value>>,
    pub doc: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub recipe: String,
}

impl Recipe {
    /// The recipe body as text, with interpolations rendered as written.
    pub fn body(&self) -> String {
        self.body
            .iter()
            .map(|line| {
                line.iter()
                    .map(|fragment| match fragment {
                        serde_json::Value::String(text) => text.clone(),
                        other => other.to_string(),
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Deserialize)]
struct Dump {
    recipes: BTreeMap<String, Recipe>,
}

/// One variable's value, evaluated. `--dump` renders an assignment as its expression tree, and
/// the tree is not the answer a consumer of the value wants.
pub fn variable(directory: &Path, name: &str) -> String {
    let output =
        Command::new("just").args(["--evaluate", name]).current_dir(directory).output().expect("just runs");
    assert!(
        output.status.success(),
        "just --evaluate {name} failed in {}: {}",
        directory.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Every recipe a Justfile declares.
pub fn recipes(directory: &Path) -> BTreeMap<String, Recipe> {
    let recipes = dump(directory).recipes;
    assert!(!recipes.is_empty(), "no recipe was found; every check over them is vacuous");
    recipes
}

fn dump(directory: &Path) -> Dump {
    let output = Command::new("just")
        .args(["--dump", "--dump-format", "json"])
        .current_dir(directory)
        .output()
        .expect("just runs");
    assert!(
        output.status.success(),
        "just --dump failed in {}: {}",
        directory.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    serde_json::from_slice(&output.stdout).expect("just --dump emits JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_body_renders_the_lines_it_runs() {
        let recipes = recipes(&crate::repo_root());
        assert!(recipes["lint-md"].body().contains("markdownlint-cli2"));
    }
}
