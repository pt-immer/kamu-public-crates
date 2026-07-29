#![allow(dead_code)]

use cargo_metadata::{Metadata, MetadataCommand};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn lane_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("hygiene crate must stay inside the extension lane")
        .to_path_buf();
    assert!(root.join("Justfile").is_file(), "{} is not the lane root", root.display());
    root
}

pub fn repository_root() -> PathBuf {
    let root = lane_root()
        .parent()
        .and_then(Path::parent)
        .expect("extensions/money-pg must stay below the repository root")
        .to_path_buf();
    assert!(
        root.join("Cargo.toml").is_file() && root.join("crates").is_dir(),
        "{} is not the repository root",
        root.display()
    );
    root
}

pub fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
}

pub fn rust_sources_under(root: &Path) -> Vec<PathBuf> {
    fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", directory.display()))
        {
            let path = entry.expect("directory entry must be readable").path();
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    files
}

pub fn tracked_files(pathspec: Option<&str>) -> Vec<PathBuf> {
    let mut command = Command::new("git");
    command.args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"]);
    if let Some(pathspec) = pathspec {
        command.arg(pathspec);
    }
    let output = command.current_dir(lane_root()).output().expect("git ls-files must run");
    assert!(output.status.success(), "git ls-files failed");
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(String::from_utf8_lossy(path).into_owned()))
        .collect()
}

pub fn metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(lane_root().join("Cargo.toml"))
        .no_deps()
        .exec()
        .expect("lane cargo metadata must resolve")
}

pub fn manifest(path: impl AsRef<Path>) -> toml::Value {
    let path = path.as_ref();
    toml::from_str(&read(path))
        .unwrap_or_else(|error| panic!("{} must parse as TOML: {error}", path.display()))
}

pub fn just_dump(root: &Path) -> Value {
    let output = Command::new("just")
        .args(["--dump", "--dump-format", "json"])
        .current_dir(root)
        .output()
        .expect("just --dump must run");
    assert!(output.status.success(), "just --dump failed: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("just --dump must return JSON")
}

pub fn recipe<'a>(dump: &'a Value, name: &str) -> &'a Value {
    dump.get("recipes")
        .and_then(|recipes| recipes.get(name))
        .unwrap_or_else(|| panic!("Justfile must define `{name}`"))
}

pub fn recipe_dependencies<'a>(dump: &'a Value, name: &str) -> Vec<&'a str> {
    recipe(dump, name)
        .get("dependencies")
        .and_then(Value::as_array)
        .expect("recipe dependencies must be an array")
        .iter()
        .filter_map(|dependency| dependency.get("recipe").and_then(Value::as_str))
        .collect()
}

pub fn recipe_body(dump: &Value, name: &str) -> String {
    fn render(fragment: &Value, output: &mut String) {
        match fragment {
            Value::String(text) => output.push_str(text),
            Value::Array(parts)
                if parts.len() == 2 && parts.first().and_then(Value::as_str) == Some("variable") =>
            {
                output.push_str("{{ ");
                output.push_str(parts[1].as_str().expect("variable fragment must name a parameter"));
                output.push_str(" }}");
            }
            Value::Array(parts) => {
                for part in parts {
                    render(part, output);
                }
            }
            _ => {}
        }
    }

    recipe(dump, name)
        .get("body")
        .and_then(Value::as_array)
        .expect("recipe body must be an array")
        .iter()
        .filter_map(|line| {
            let mut rendered = String::new();
            render(line, &mut rendered);
            (!rendered.trim_start().starts_with('#')).then_some(rendered)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn recipe_empty_parameters(dump: &Value, name: &str) -> Vec<String> {
    recipe(dump, name)
        .get("parameters")
        .and_then(Value::as_array)
        .expect("recipe parameters must be an array")
        .iter()
        .filter(|parameter| parameter.get("default").and_then(Value::as_str) == Some(""))
        .filter_map(|parameter| parameter.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect()
}
