//! Every pinned version has one home, and the copies a tool insists on are held equal to it.

use repo_policy::actions::sources as actions_sources;
use repo_policy::dev_tools::DevTools;
use repo_policy::{read, repo_root, tracked};

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
    assert_eq!(tools().rust.primary, channel("rust-toolchain.toml"));
}

#[test]
fn the_lane_channel_is_the_one_rustup_selects_inside_the_lane() {
    assert_eq!(tools().rust.lane, channel("extensions/money-pg/rust-toolchain.toml"));
}

/// A manifest's own `rust-version`. Absent and unreadable are different answers: a floor
/// written as a bare TOML number would decode to no string, and skipping it would leave it
/// bound to nothing while the scan reported success.
fn declared_floor(relative: &str) -> Option<String> {
    let document = toml_value(relative);
    let stated = document
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .or_else(|| document.get("package").and_then(|package| package.get("rust-version")));
    match stated {
        None => None,
        Some(toml::Value::String(version)) => Some(version.clone()),
        // `rust-version.workspace = true` parses as a table and states no floor of its own.
        Some(toml::Value::Table(table)) if table.get("workspace") == Some(&toml::Value::Boolean(true)) => {
            None
        }
        Some(other) => {
            panic!("{relative} states rust-version as {other}, which is neither a version nor an inheritance")
        }
    }
}

/// The public workspace and the excluded lane declare different floors, each owned elsewhere.
/// Listing the manifests that state one is what goes stale, so every tracked manifest is read
/// and every literal has to be placed on one side or the other.
#[test]
fn every_declared_floor_belongs_to_a_side_that_owns_it() {
    let manifest = tools();
    let (mut public, mut lane) = (0_usize, 0_usize);

    for relative in tracked(&["*Cargo.toml"]) {
        let Some(declared) = declared_floor(&relative) else {
            continue;
        };
        if relative.starts_with("extensions/money-pg/") {
            // Held equal to the lane's own clippy.toml by the lane's hygiene crate. Counted
            // here so a lane manifest cannot pass by going unread.
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

fn listed(relative: &str, key: &str) -> Vec<String> {
    toml_value(relative)["toolchain"]
        .get(key)
        .and_then(toml::Value::as_array)
        .map(|values| values.iter().filter_map(|value| value.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

#[test]
fn each_toolchain_file_carries_what_the_manifest_says_its_side_installs() {
    let manifest = tools();
    for (relative, components, targets) in [
        ("rust-toolchain.toml", &manifest.rust.primary_components, Some(&manifest.rust.primary_targets)),
        ("extensions/money-pg/rust-toolchain.toml", &manifest.rust.lane_components, None),
    ] {
        let present = listed(relative, "components");
        assert!(!present.is_empty(), "{relative} lists no component to bind");
        assert!(!components.is_empty(), "{} names no component for {relative}", DevTools::PATH);
        for component in components {
            assert!(present.contains(component), "{relative} omits the component {component}");
        }
        if let Some(targets) = targets {
            let present = listed(relative, "targets");
            assert!(!present.is_empty(), "{relative} lists no target to bind");
            for target in targets {
                assert!(present.contains(target), "{relative} omits the target {target}");
            }
        }
    }
}

/// Every toolchain a file selects, one entry per selection. A matrix states several on one
/// line, and `RUSTUP_TOOLCHAIN` selects one without naming `toolchain:` at all.
fn selected_toolchains() -> Vec<(String, String)> {
    let mut selections = Vec::new();
    for (name, text) in actions_sources() {
        for line in text.lines() {
            let trimmed = line.trim_start();
            let value = trimmed
                .strip_prefix("toolchain:")
                .or_else(|| trimmed.strip_prefix("RUSTUP_TOOLCHAIN:"))
                .map(str::trim);
            let Some(value) = value else { continue };
            let inner = value.strip_prefix('[').and_then(|rest| rest.strip_suffix(']'));
            match inner {
                Some(list) => {
                    selections.extend(list.split(',').map(|entry| (name.clone(), entry.trim().to_owned())))
                }
                None => selections.push((name.clone(), value.to_owned())),
            }
        }
    }
    assert!(!selections.is_empty(), "no toolchain selection to check");
    selections
}

fn unquoted(value: &str) -> &str {
    value.trim_matches(['"', '\''])
}

/// A version literal is a copy of a pin, and the copies are what a bump has to reach. Scanning
/// the form rather than the sites is what makes this total.
#[test]
fn nothing_actions_runs_selects_a_toolchain_by_literal_version() {
    for (source, value) in selected_toolchains() {
        assert!(
            !unquoted(&value).starts_with(|character: char| character.is_ascii_digit()),
            "{source} selects the toolchain by the literal {value}; \
             read the manifest through the read-dev-tools action instead",
        );
    }
}

/// The complement: a value that is neither a literal nor a reference would satisfy the scan
/// above while selecting nothing this repository pins.
#[test]
fn every_selected_toolchain_is_a_reference_or_a_named_channel() {
    let mut referenced = 0_usize;
    for (source, value) in selected_toolchains() {
        let value = unquoted(&value);
        let accepted = value.contains("${{") || matches!(value, "stable" | "nightly");
        assert!(
            accepted,
            "{source} selects {value}, which is neither a manifest reference nor a named channel"
        );
        if value.contains("${{") {
            referenced += 1;
        }
    }
    assert!(referenced > 0, "nothing reads a pin; this would pass vacuously");
}

/// The matrix leg that exists to compile at the floor has to read the floor. Pointed at any
/// other pin it still runs, still passes, and stops testing the version every published crate
/// promises.
#[test]
fn the_toolchain_matrix_compiles_at_the_declared_floor() {
    let mut matrices = 0_usize;
    for (source, text) in actions_sources() {
        for line in text.lines() {
            let trimmed = line.trim_start();
            let Some(value) = trimmed.strip_prefix("toolchain:").map(str::trim) else {
                continue;
            };
            let Some(list) = value.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) else {
                continue;
            };
            matrices += 1;
            let reads_msrv = list.split(',').filter(|entry| entry.contains("rust_msrv")).count();
            assert_eq!(
                1, reads_msrv,
                "{source} declares the toolchain matrix {value}, which reads the msrv pin \
                 {reads_msrv} times; exactly one leg must compile at the floor",
            );
        }
    }
    assert!(matrices > 0, "no toolchain matrix found; this would pass vacuously");
}

fn outputs_block(text: &str, source: &str) -> String {
    text.split_once("\noutputs:\n")
        .unwrap_or_else(|| panic!("{source} declares no outputs"))
        .1
        .split("\nruns:")
        .next()
        .expect("split always yields a first part")
        .to_owned()
}

/// An output's name promises which pin it carries. Nothing else checks the expression under
/// it, and every job in the repository reaches the manifest through exactly these three.
#[test]
fn each_action_output_reads_the_manifest_key_its_name_states() {
    let source = ".github/actions/read-dev-tools/action.yml";
    let text = read(source);
    let block = outputs_block(&text, source);

    let mut bound = 0_usize;
    let mut name = String::new();
    for line in block.lines() {
        if let Some(declared) = line.strip_prefix("  ").and_then(|rest| rest.strip_suffix(':')) {
            name = declared.to_owned();
            continue;
        }
        let Some(value) = line.trim_start().strip_prefix("value:").map(str::trim) else {
            continue;
        };
        let key = name
            .strip_prefix("rust_")
            .unwrap_or_else(|| panic!("{source} declares the output {name}, which names no pin"));
        let expected = format!("${{{{ fromJSON(steps.read.outputs.manifest).rust.{key} }}}}");
        assert_eq!(value, expected, "{source} output {name} reads {value}");
        bound += 1;
    }
    assert!(bound > 0, "no action output was bound; this would pass vacuously");
}

/// The job republishes the action's outputs so every other job can reach them. A pin renamed
/// on the way through would be read under a name that no longer describes it.
#[test]
fn the_changes_job_republishes_each_pin_under_its_own_name() {
    let source = ".github/workflows/on-pr-synced.yml";
    let text = read(source);
    let declared: Vec<(String, String)> = text
        .lines()
        .filter_map(|line| line.strip_prefix("      "))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .filter(|(name, _)| name.starts_with("rust_"))
        .collect();

    assert!(!declared.is_empty(), "the changes job republishes no pin");
    for (name, value) in declared {
        assert_eq!(
            value,
            format!("${{{{ steps.pins.outputs.{name} }}}}"),
            "the changes job publishes {name} as {value}",
        );
    }
}
