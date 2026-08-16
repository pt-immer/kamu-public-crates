//! Every pinned version has one home, and the copies a tool insists on are held equal to it.

use repo_policy::actions::{composite_actions, entries_at, sources as actions_sources};
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
    // The error names the path and the reason; `expect` would replace both with this string.
    DevTools::load(&repo_root()).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn the_primary_channel_is_the_one_rustup_selects_at_the_repository_root() {
    assert_eq!(tools().rust.primary, channel("rust-toolchain.toml"));
}

#[test]
fn the_lane_channel_is_the_one_rustup_selects_inside_the_lane() {
    assert_eq!(tools().rust.lane, channel("extensions/money-pg/rust-toolchain.toml"));
}

/// How a manifest answers the question "what is your floor".
enum Floor {
    /// Stated here, as a version.
    Stated(String),
    /// Taken from the workspace, via `rust-version.workspace = true`.
    Inherited,
    /// Not answered at all. A package with no floor compiles against whatever is installed.
    Absent,
}

fn floor_of(relative: &str) -> Option<Floor> {
    let document = toml_value(relative);
    // A workspace root answers for its members through `[workspace.package]`; a package
    // answers for itself. A manifest that is neither answers nothing and is not asked.
    if document.get("package").is_none() && document.get("workspace").is_none() {
        return None;
    }
    let stated = document
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .or_else(|| document.get("package").and_then(|package| package.get("rust-version")));
    Some(match stated {
        None => Floor::Absent,
        Some(toml::Value::String(version)) => Floor::Stated(version.clone()),
        Some(toml::Value::Table(table)) if table.get("workspace") == Some(&toml::Value::Boolean(true)) => {
            Floor::Inherited
        }
        Some(other) => {
            panic!("{relative} states rust-version as {other}, which is neither a version nor an inheritance")
        }
    })
}

/// Every package answers the floor question, and every literal answer belongs to a side that
/// owns it. Listing the manifests that state one is what goes stale; a package that answers
/// nothing compiles against whatever toolchain happens to be installed, which is the same
/// silence in a different place.
#[test]
fn every_package_has_a_floor_and_every_literal_belongs_to_a_side() {
    let manifest = tools();
    let (mut public, mut lane, mut inherited) = (0_usize, 0_usize, 0_usize);

    for relative in tracked(&["*Cargo.toml"]) {
        let Some(floor) = floor_of(&relative) else {
            continue;
        };
        let declared = match floor {
            Floor::Inherited => {
                inherited += 1;
                continue;
            }
            Floor::Absent => panic!(
                "{relative} declares a package with no rust-version; it would compile against \
                 whatever toolchain is installed",
            ),
            Floor::Stated(version) => version,
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
    assert!(inherited > 0, "no manifest inherited a floor; the workspace case would be untested");
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
    // Counting any expression would let `${{ matrix.toolchain }}`, which this workflow already
    // contains, satisfy the guard while nothing read a pin at all.
    let mut referenced = 0_usize;
    for (source, value) in selected_toolchains() {
        let value = unquoted(&value);
        let reads_pin = value.contains("outputs.rust_");
        if reads_pin {
            referenced += 1;
        }
        // Any `${{ }}` would let `${{ env.ANYTHING }}` read as a manifest reference. The
        // matrix is accepted because the matrix itself is checked to read a pin.
        let accepted =
            reads_pin || value.contains("matrix.toolchain") || matches!(value, "stable" | "nightly");
        assert!(accepted, "{source} selects {value}, which reads no pin this repository states");
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

/// An output's name promises which pin it carries, and nothing else checks the expression
/// under it. Every composite action is read, not the one this branch happens to have added.
#[test]
fn each_action_output_reads_the_manifest_key_its_name_states() {
    let mut bound = 0_usize;
    for (source, text) in composite_actions() {
        let Some((_, rest)) = text.split_once("\noutputs:\n") else {
            continue;
        };
        let block = rest.split("\nruns:").next().expect("split yields a first part");
        // An output name sits at exactly two spaces; `value:` under it sits deeper. Tracking
        // "the last line ending in a colon" would let a block-form `description:` rebind it.
        let names: Vec<String> = entries_at(block, 2).into_iter().map(|(name, _)| name).collect();
        let values: Vec<String> =
            entries_at(block, 4).into_iter().filter(|(key, _)| key == "value").map(|(_, v)| v).collect();
        assert_eq!(
            names.len(),
            values.len(),
            "{source} declares {} outputs and {} values",
            names.len(),
            values.len(),
        );
        for (name, value) in names.iter().zip(values) {
            let key = name
                .strip_prefix("rust_")
                .unwrap_or_else(|| panic!("{source} declares the output {name}, which names no pin"));
            let expected = format!("${{{{ fromJSON(steps.read.outputs.manifest).rust.{key} }}}}");
            assert_eq!(value, expected, "{source} output {name} reads {value}");
            bound += 1;
        }
    }
    assert!(bound > 0, "no action output was bound; this would pass vacuously");
}

/// A job republishes the action's outputs so other jobs can reach them. A pin renamed on the
/// way through would be read under a name that no longer describes it. Every workflow is
/// scanned, not the one that republishes them today.
#[test]
fn every_republished_pin_keeps_its_own_name() {
    let mut republished = 0_usize;
    for (source, text) in actions_sources() {
        for (name, value) in entries_at(&text, 6) {
            if !name.starts_with("rust_") {
                continue;
            }
            let Some(read_name) =
                value.rsplit_once(".outputs.").map(|(_, tail)| tail.trim_end_matches([' ', '}']).to_owned())
            else {
                panic!("{source} publishes {name} as {value}, which reads no output");
            };
            assert_eq!(name, read_name, "{source} publishes {name} from {value}");
            republished += 1;
        }
    }
    assert!(republished > 0, "no pin was republished; this would pass vacuously");
}
