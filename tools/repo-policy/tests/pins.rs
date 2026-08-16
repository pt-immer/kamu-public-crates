//! Every pinned version has one home, and the copies a tool insists on are held equal to it.

use std::path::{Path, PathBuf};

use repo_policy::dev_tools::DevTools;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two levels below the repository root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {relative}: {error}"))
}

fn toml_value(relative: &str) -> toml::Value {
    toml::from_str(&read(relative)).unwrap_or_else(|error| panic!("cannot parse {relative}: {error}"))
}

fn channel(relative: &str) -> String {
    toml_value(relative)["toolchain"]["channel"]
        .as_str()
        .unwrap_or_else(|| panic!("{relative} declares no toolchain channel"))
        .to_owned()
}

/// `1.94` and `1.94.0` are the same floor written two ways, so compare the numbers.
fn series(version: &str, source: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| {
            part.parse().unwrap_or_else(|_| panic!("{source} states {version}, which is not a version"))
        })
        .collect()
}

fn same_series(left: &str, left_source: &str, right: &str, right_source: &str) -> bool {
    let (mut a, mut b) = (series(left, left_source), series(right, right_source));
    let width = a.len().max(b.len());
    a.resize(width, 0);
    b.resize(width, 0);
    a == b
}

fn tools() -> DevTools {
    DevTools::load(&repo_root()).expect("the pinned-version manifest decodes")
}

#[test]
fn the_primary_channel_is_the_one_rustup_selects_at_the_repository_root() {
    let manifest = tools();
    assert_eq!(manifest.rust.primary, channel("rust-toolchain.toml"));
}

#[test]
fn the_lane_channel_is_the_one_rustup_selects_inside_the_lane() {
    let manifest = tools();
    assert_eq!(manifest.rust.lane, channel("extensions/money-pg/rust-toolchain.toml"),);
}

/// Every path git tracks, so a scan covers what the repository actually contains rather than
/// what someone remembered to list.
fn tracked(pattern: &str) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", pattern])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files runs");
    assert!(output.status.success(), "git ls-files failed for {pattern}");
    let listed: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!listed.is_empty(), "git tracks nothing matching {pattern}");
    listed
}

/// A manifest's own `rust-version`, or `None` when it inherits the workspace's.
fn declared_floor(relative: &str) -> Option<String> {
    let document = toml_value(relative);
    document
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .or_else(|| document.get("package").and_then(|package| package.get("rust-version")))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
}

/// The public workspace and the excluded lane declare different floors, and each is owned
/// elsewhere. Listing the manifests that state one is what goes stale, so every tracked
/// manifest is read and every literal has to be placed on one side or the other.
#[test]
fn every_declared_floor_belongs_to_a_side_that_owns_it() {
    let manifest = tools();
    let mut public = 0_usize;
    let mut lane = 0_usize;

    for relative in tracked("*Cargo.toml") {
        let Some(declared) = declared_floor(&relative) else {
            continue;
        };
        if relative.starts_with("extensions/money-pg/") {
            // The lane's floor is held equal to its own clippy.toml by the lane's hygiene
            // crate. Counted here so a lane manifest cannot pass by going unread.
            lane += 1;
            continue;
        }
        assert!(
            same_series(&manifest.rust.msrv, DevTools::PATH, &declared, &relative),
            "{} states msrv {} while {relative} declares rust-version {declared}",
            DevTools::PATH,
            manifest.rust.msrv,
        );
        public += 1;
    }

    assert!(public > 0, "no public manifest declared a floor to bind");
    assert!(lane > 0, "no lane manifest was seen; the side split would be untested");
}

#[test]
fn the_root_toolchain_file_carries_every_component_and_target_the_manifest_names() {
    let manifest = tools();
    let toolchain = toml_value("rust-toolchain.toml");
    let listed = |key: &str| -> Vec<String> {
        toolchain["toolchain"]
            .get(key)
            .and_then(toml::Value::as_array)
            .map(|values| values.iter().filter_map(|value| value.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    };

    let components = listed("components");
    let targets = listed("targets");
    assert!(!components.is_empty(), "no component to bind");
    assert!(!targets.is_empty(), "no target to bind");

    for component in &manifest.rust.primary_components {
        assert!(components.contains(component), "rust-toolchain.toml omits the component {component}",);
    }
    for target in &manifest.rust.primary_targets {
        assert!(targets.contains(target), "rust-toolchain.toml omits the target {target}",);
    }
}

/// Everything Actions executes: the workflows, and the composite actions they call. A pin
/// restated in an action is the same copy as one restated in a workflow.
fn workflow_sources() -> Vec<(String, String)> {
    let mut sources: Vec<(String, String)> = tracked(".github/workflows/*.yml")
        .into_iter()
        .chain(tracked(".github/actions/*/action.yml"))
        .map(|relative| {
            let text = read(&relative);
            (relative, text)
        })
        .collect();
    sources.sort();
    assert!(!sources.is_empty(), "nothing to scan");
    sources
}

/// Every `toolchain:` a workflow selects, as `(workflow, value)`.
fn selected_toolchains() -> Vec<(String, String)> {
    workflow_sources()
        .into_iter()
        .flat_map(|(name, text)| {
            text.lines()
                .filter_map(|line| {
                    line.trim_start().strip_prefix("toolchain:").map(|value| value.trim().to_owned())
                })
                .map(move |value| (name.clone(), value))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A version literal in a workflow is a copy of a pin, and the copies are what a bump has to
/// reach. Scanning the form rather than listing the sites is what makes this total: a literal
/// added tomorrow, anywhere, in any workflow, fails here.
#[test]
fn no_workflow_selects_a_toolchain_by_literal_version() {
    let selections = selected_toolchains();
    assert!(!selections.is_empty(), "no toolchain selection to check");

    for (workflow, value) in selections {
        let unquoted = value.trim_matches(['"', '\'']);
        assert!(
            !unquoted.starts_with(|character: char| character.is_ascii_digit()),
            "{workflow} selects the toolchain by the literal {value}; \
             reference the manifest through the read-dev-tools action instead",
        );
    }
}

/// The complement of the scan above: a value that is neither a literal nor a reference would
/// satisfy it while selecting nothing this repository pins.
#[test]
fn every_selected_toolchain_is_a_reference_or_the_named_nightly() {
    let mut referenced = 0_usize;
    for (workflow, value) in selected_toolchains() {
        let accepted = value.contains("${{") || value == "nightly";
        assert!(accepted, "{workflow} selects {value}, which is neither a manifest reference nor nightly",);
        if value.contains("${{") {
            referenced += 1;
        }
    }
    assert!(referenced > 0, "no workflow reads a pin; this would pass vacuously");
}
