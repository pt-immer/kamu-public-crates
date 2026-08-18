//! The Justfile, read through `just --dump` rather than parsed again.

use std::collections::{BTreeMap, BTreeSet};
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

/// Every recipe a Justfile declares.
pub fn recipes(directory: &Path) -> BTreeMap<String, Recipe> {
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
    let dump: Dump = serde_json::from_slice(&output.stdout).expect("just --dump emits JSON");
    assert!(!dump.recipes.is_empty(), "no recipe was found; every check over them is vacuous");
    dump.recipes
}

/// Root recipes that run inside the extension lane, transitively.
///
/// `just pg` is not the only way in — `gate-pg` cds there too and `gate-all` composes it — so a
/// hand-written marker would be a claim about the Justfile rather than a reading of it.
pub fn lane_entry_recipes() -> BTreeSet<String> {
    let recipes = recipes(&crate::repo_root());
    let mut entries: BTreeSet<String> = recipes
        .values()
        .filter(|recipe| recipe.body().contains("cd extensions/money-pg"))
        .map(|recipe| recipe.name.clone())
        .collect();

    loop {
        let reached: BTreeSet<String> = recipes
            .values()
            .filter(|recipe| {
                !entries.contains(&recipe.name)
                    && recipe.dependencies.iter().any(|need| entries.contains(&need.recipe))
            })
            .map(|recipe| recipe.name.clone())
            .collect();
        if reached.is_empty() {
            break;
        }
        entries.extend(reached);
    }

    assert!(!entries.is_empty(), "no root recipe enters the extension lane; re-point this derivation");
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lane_entry_set_is_derived_not_listed() {
        let entries = lane_entry_recipes();
        assert!(entries.contains("pg"), "just pg cds into the lane");
        assert!(entries.contains("gate-pg"), "gate-pg cds into the lane");
        assert!(entries.contains("gate-all"), "gate-all composes gate-pg");
        assert!(!entries.contains("gate"), "the public gate does not enter the lane");
    }

    #[test]
    fn a_recipe_body_renders_the_lines_it_runs() {
        let recipes = recipes(&crate::repo_root());
        assert!(recipes["gate-pg"].body().contains("cd extensions/money-pg"));
    }
}
