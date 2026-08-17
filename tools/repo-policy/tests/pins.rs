//! Every pinned version has one home, and the copies a tool insists on are held equal to it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex_lite::Regex;

use repo_policy::actions::{
    ActionOutput, INSTALL_ACTION, MANIFEST_ACTION, MANIFEST_OUTPUT, action_outputs, code_of,
    install_action_steps, job_outputs, manifest_paths, needs_output_references, sources as actions_sources,
    step_scopes, step_uses, tool_requests,
};
use repo_policy::dev_tools::{DevTools, TOOL_SECTION_SUFFIX};
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

/// The manifest as written, for asking whether a key exists at all. The typed decoder answers
/// what the fields it models say; it cannot answer for a key nothing models.
fn manifest_json() -> serde_json::Value {
    serde_json::from_str(&read(DevTools::PATH))
        .unwrap_or_else(|error| panic!("cannot parse {}: {error}", DevTools::PATH))
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

/// The root toolchain file carries what the root `setup` installs. The lane's carries its own,
/// and the lane installs it from there, so the manifest states the lane's channel and nothing
/// else about it.
#[test]
fn the_root_toolchain_file_carries_what_the_manifest_says_setup_installs() {
    let manifest = tools();
    let relative = "rust-toolchain.toml";

    let components = listed(relative, "components");
    assert!(!components.is_empty(), "{relative} lists no component to bind");
    assert!(!manifest.rust.primary_components.is_empty(), "{} names no component", DevTools::PATH);
    for component in &manifest.rust.primary_components {
        assert!(components.contains(component), "{relative} omits the component {component}");
    }

    let targets = listed(relative, "targets");
    assert!(!targets.is_empty(), "{relative} lists no target to bind");
    for target in &manifest.rust.primary_targets {
        assert!(targets.contains(target), "{relative} omits the target {target}");
    }
}

/// Every toolchain a file selects, one entry per selection. A matrix states several on one
/// line, and `RUSTUP_TOOLCHAIN` selects one without naming `toolchain:` at all.
fn selected_toolchains() -> Vec<(String, String)> {
    let mut selections = Vec::new();
    for source in actions_sources() {
        let (name, text) = (&source.path, &source.text);
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
        let reads_pin =
            manifest_paths(value).iter().any(|path| path.first().is_some_and(|key| key == "rust"));
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
///
/// This governs public-workspace matrices. A lane job may not carry one at all, which
/// `scripts/test_workflows.py` refuses separately, because a matrix there would install the
/// public workspace's floor into the lane.
#[test]
fn the_toolchain_matrix_compiles_at_the_declared_floor() {
    let mut matrices = 0_usize;
    for entry in actions_sources() {
        let (source, text) = (&entry.path, &entry.text);
        for line in text.lines() {
            let trimmed = line.trim_start();
            let Some(value) = trimmed.strip_prefix("toolchain:").map(str::trim) else {
                continue;
            };
            let Some(list) = value.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) else {
                continue;
            };
            matrices += 1;
            let msrv = vec!["rust".to_owned(), "msrv".to_owned()];
            let reads_msrv = list.split(',').filter(|entry| manifest_paths(entry).contains(&msrv)).count();
            assert_eq!(
                1, reads_msrv,
                "{source} declares the toolchain matrix {value}, which reads the msrv pin \
                 {reads_msrv} times; exactly one leg must compile at the floor",
            );
        }
    }
    assert!(matrices > 0, "no toolchain matrix found; this would pass vacuously");
}

/// The tool sections, refusing a pin stated twice.
///
/// Three ways one can be. A key repeated inside a section, a section repeated at the top
/// level, and one tool defined by two different sections. Every reader keeps the last of a
/// repeated key and reports nothing -- `serde_json`, Python's `json`, and Actions' `fromJSON`
/// alike -- so the file states a version it does not unambiguously carry, and the pin that
/// reaches CI is whichever section a reader looked at first.
///
/// Read from the TEXT. Handing this a `serde_json::Value` would hand it a document whose
/// duplicate had already been collapsed by the parse that produced it, and the check could
/// never fail.
struct DistinctPins;

/// The keys one section states, refusing a repeat within it.
struct SectionKeys(Vec<String>);

impl<'de> serde::Deserialize<'de> for SectionKeys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Keys;

        impl<'de> serde::de::Visitor<'de> for Keys {
            type Value = SectionKeys;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a tool section")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<SectionKeys, M::Error> {
                let mut names = Vec::new();
                while let Some(name) = map.next_key::<String>()? {
                    map.next_value::<serde::de::IgnoredAny>()?;
                    if names.contains(&name) {
                        return Err(serde::de::Error::custom(format!("{name} is stated twice")));
                    }
                    names.push(name);
                }
                Ok(SectionKeys(names))
            }
        }

        deserializer.deserialize_map(Keys)
    }
}

impl<'de> serde::Deserialize<'de> for DistinctPins {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Top;

        impl<'de> serde::de::Visitor<'de> for Top {
            type Value = DistinctPins;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("the pinned-version manifest")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<DistinctPins, M::Error> {
                let mut sections = BTreeSet::new();
                let mut tools: BTreeMap<String, String> = BTreeMap::new();
                while let Some(name) = map.next_key::<String>()? {
                    if !sections.insert(name.clone()) {
                        return Err(serde::de::Error::custom(format!("{name} is stated twice")));
                    }
                    // Every tool section, found by shape rather than listed here.
                    if !name.ends_with(TOOL_SECTION_SUFFIX) {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        continue;
                    }
                    let SectionKeys(entries) = map
                        .next_value::<SectionKeys>()
                        .map_err(|error| serde::de::Error::custom(format!("{name}: {error}")))?;
                    for tool in entries {
                        if let Some(first) = tools.insert(tool.clone(), name.clone()) {
                            return Err(serde::de::Error::custom(format!(
                                "{tool} is pinned by both {first} and {name}"
                            )));
                        }
                    }
                }
                if tools.is_empty() {
                    return Err(serde::de::Error::custom("no tool section was read"));
                }
                Ok(DistinctPins)
            }
        }

        deserializer.deserialize_map(Top)
    }
}

#[test]
fn no_tool_is_pinned_twice() {
    serde_json::from_str::<DistinctPins>(&read(DevTools::PATH))
        .unwrap_or_else(|error| panic!("{}: {error}", DevTools::PATH));
}

/// A job that republishes the manifest has to republish the one the read-dev-tools action
/// produced. Nothing downstream can tell: every consumer indexes a path, and the paths are
/// checked against the file rather than against what the job actually carried. Pointed at a
/// step that publishes no such output, the expression yields the empty string, `fromJSON`
/// receives it, and the pins every job reads are gone while the gate reports green.
#[test]
fn every_republished_manifest_comes_from_the_action_that_reads_it() {
    let published: BTreeSet<(&str, String, String)> = job_outputs()
        .into_iter()
        .filter(|(_, _, name, _)| name == MANIFEST_OUTPUT)
        .map(|(source, job, _, value)| {
            let read = needs_output_references(&value);
            assert!(read.is_empty(), "{source} job {job} republishes the manifest from another job");
            let step = value
                .split_once("steps.")
                .and_then(|(_, tail)| tail.split_once(".outputs."))
                .map(|(id, _)| id.to_owned())
                .unwrap_or_else(|| {
                    panic!("{source} job {job} publishes the manifest as {value}, which reads no step")
                });
            (source, job.clone(), step)
        })
        .collect();
    assert!(!published.is_empty(), "no job republishes the manifest; this would pass vacuously");

    let reading: BTreeSet<(&str, String, String)> = step_uses()
        .into_iter()
        .filter(|(_, _, _, clause)| clause.contains("read-dev-tools"))
        .map(|(source, scope, id, _)| (source, scope, id))
        .collect();

    for entry in &published {
        let (source, job, step) = entry;
        assert!(
            reading.contains(entry),
            "{source} job {job} republishes the manifest from step {step}, which does not run \
             the read-dev-tools action",
        );
    }
}

/// No Actions source states a version literal outside a comment. Every version this repository
/// pins is indexed out of the manifest, so a literal is a copy -- and a stale copy reads as
/// correct. The comment beside an action's commit pin is the one place a version belongs,
/// because it names what that commit is and no expression can.
///
/// This covers what the tool rule cannot see: a cache key carrying a version reaches no
/// `tool:` request, so before this a stale one passed the whole public gate.
#[test]
fn no_actions_source_states_a_version_literal() {
    let mut scanned = 0_usize;
    for source in actions_sources() {
        for (number, line) in source.text.lines().enumerate() {
            if let Some(found) = version_literal(code_of(line)) {
                panic!(
                    "{}:{} states the version literal {found}; index it out of the manifest \
                     the read-dev-tools action publishes, or -- if this repository pins no \
                     such version -- give it a home there first",
                    source.path,
                    number + 1,
                );
            }
        }
        scanned += 1;
    }
    assert!(scanned > 0, "no Actions source was scanned; this would pass vacuously");
}

/// The first version literal in a line, if any.
///
/// Exactly three components, which is what every version this repository pins is, and what a
/// dotted address is not: `127.0.0.1` and `0.0.0.0` are four, and a `run:` line is entitled to
/// carry one. Fewer than three reads the same as an ordinary decimal. A trailing dot ends a
/// sentence rather than a version and is outside the match by construction.
fn version_literal(line: &str) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            Regex::new(r"(?:^|[^0-9.])([0-9]+\.[0-9]+\.[0-9]+)(?:[^0-9.]|$)").expect("the pattern compiles")
        })
        .captures(line)
        .map(|found| found[1].to_owned())
}

/// Every manifest path a workflow indexes has to exist. A path Actions cannot resolve is not
/// an error there: it yields the empty string, so the job installs whatever the runner already
/// had and the gate stays green.
///
/// This is what replaced a rule about output names. Nothing is published per pin any more, so
/// there is no name left to keep honest -- only reads, and whether the document answers them.
#[test]
fn every_manifest_path_a_workflow_reads_exists() {
    let document = manifest_json();
    let mut read = 0_usize;
    for source in actions_sources() {
        // Per line and past the comment, as the literal scan reads them. Whole-file text would
        // check a path written in a comment as though Actions resolved it.
        for (number, line) in source.text.lines().enumerate() {
            let code = code_of(line);
            let paths = manifest_paths(code);
            // Every call accounted for. A read the parser cannot read yields no path, and a check
            // that only counts what it parsed cannot tell that from a line with no read on it.
            assert_eq!(
                paths.len(),
                code.matches("fromJSON(").count(),
                "{}:{} writes a manifest read this cannot parse; it would be checked by nothing",
                source.path,
                number + 1,
            );
            for path in paths {
                let mut value = &document;
                for segment in &path {
                    value = value.get(segment).unwrap_or_else(|| {
                        panic!(
                            "{} indexes {}, and {} states no {segment} there",
                            source.path,
                            path.join("."),
                            DevTools::PATH,
                        )
                    });
                }
                assert!(
                    value.is_string(),
                    "{} indexes {}, which is not a version in {}",
                    source.path,
                    path.join("."),
                    DevTools::PATH,
                );
                read += 1;
            }
        }
    }
    assert!(read > 0, "no manifest path was read; this would pass vacuously");
}

/// A job reading `needs.<job>.outputs.manifest` names a job that publishes it.
///
/// Depending on a job is not the same as that job answering: the branch bound a step output to
/// the step that writes it, and an action output to its own step, and left the rung between
/// them open. A read of an output no job declares is the empty string, and every pin indexed
/// out of it is gone while the gate stays green.
#[test]
fn every_manifest_a_job_reads_is_published_by_the_job_it_names() {
    let published: BTreeSet<(&str, String)> = job_outputs()
        .into_iter()
        .filter(|(_, _, name, _)| name == MANIFEST_OUTPUT)
        .map(|(source, job, _, _)| (source, job))
        .collect();
    assert!(!published.is_empty(), "no job publishes the manifest; this would pass vacuously");

    let mut checked = 0_usize;
    for scope in step_scopes() {
        for expression in &scope.expressions {
            for (job, name) in needs_output_references(expression) {
                if name != MANIFEST_OUTPUT {
                    continue;
                }
                assert!(
                    published.contains(&(scope.source, job.clone())),
                    "{} job {} reads needs.{job}.outputs.{name}, and {job} publishes no {name}",
                    scope.source,
                    scope.name,
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no job read the manifest; this would pass vacuously");
}

/// The name the manifest is published under is spelled by every reader of an expression, and
/// only one of them can import it. Renaming the action's output leaves the readers that cannot
/// recognising no reads and reporting nothing.
#[test]
fn every_reader_of_a_manifest_expression_spells_the_same_output_name() {
    let expected = format!("outputs.{MANIFEST_OUTPUT}");

    // The Actions side is derived rather than listed: anything running the action is a reader by
    // construction, so a workflow added later is covered without anyone remembering to add it.
    let mut readers: Vec<String> = tracked(&["*.yml"])
        .into_iter()
        .filter(|relative| relative.starts_with(".github/"))
        .filter(|relative| read(relative).contains(MANIFEST_ACTION))
        .collect();

    // The two readers that parse the expression from outside Actions. Neither can import the
    // constant, and nothing in their content ties them to the action except this name, so they
    // are the one part of the set that has to be stated.
    readers.push("scripts/test_workflows.py".to_owned());
    readers.push("extensions/money-pg/hygiene/tests/pins.rs".to_owned());

    for relative in &readers {
        // Escapes dropped, because one reader spells the name inside a regex as `\.`; and read
        // line by line, because a comment mentioning the old name would otherwise answer for a
        // file whose parsing no longer names it at all.
        let text = read(relative).replace('\\', "");
        let names = text.lines().any(|line| code_of(line).contains(&expected));
        assert!(names, "{relative} reads the manifest under some other name than {expected}",);
    }
    assert!(readers.len() > 2, "no workflow ran the action; this would pass vacuously");
}

/// A pin is a floor unless the entry says why it must be exact. An exact pin with no reason
/// is an exception nobody can weigh, and the reason belongs at the pin rather than in the
/// document that explains the mechanism.
#[test]
fn every_exact_pin_states_why_it_is_exact() {
    let manifest = tools();
    let mut exact = 0_usize;
    let mut floors = 0_usize;
    for (section, entries) in &manifest.tools {
        for (name, tool) in entries {
            match tool.exact.as_deref() {
                None => floors += 1,
                Some(reason) => {
                    assert!(
                        reason.split_whitespace().count() >= 3,
                        "{}: {section}.{name} is exact and states no reason worth reading",
                        DevTools::PATH,
                    );
                    exact += 1;
                }
            }
        }
    }
    assert!(exact > 0, "no pin is exact; this would pass vacuously");
    assert!(floors > 0, "every pin is exact; the floor class would be untested");
}

/// What identifies a tool section is stated in both languages that read the manifest. Two
/// readers of one document disagreeing quietly is what this branch keeps finding.
#[test]
fn both_readers_of_the_manifest_agree_what_a_tool_section_is() {
    let python = read("scripts/dev_environment.py");
    let stated = python
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("TOOL_SECTION_SUFFIX = ")?
                .trim()
                .strip_prefix('"')?
                .strip_suffix('"')
                .map(str::to_owned)
        })
        .expect("scripts/dev_environment.py states TOOL_SECTION_SUFFIX");
    assert_eq!(
        stated, TOOL_SECTION_SUFFIX,
        "scripts/dev_environment.py and repo-policy disagree on what names a tool section",
    );
}

/// The guide states which root workspace members are published. A count went stale by being a
/// count; an invariant nothing reads goes stale the same way, one member later.
#[test]
fn the_guide_names_every_unpublished_workspace_member() {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(repo_root())
        .output()
        .expect("cargo metadata runs");
    assert!(output.status.success(), "cargo metadata failed");
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON");
    let packages = metadata["packages"].as_array().expect("metadata lists packages");
    assert!(!packages.is_empty(), "the workspace has no member; this would pass vacuously");

    // `publish = false` is an empty registry list; anything else may be published. The
    // directory is what the guide names, because that is what a reader opens.
    let root = repo_root();
    let unpublished: BTreeSet<String> = packages
        .iter()
        .filter(|package| package["publish"].as_array().is_some_and(Vec::is_empty))
        .map(|package| {
            let manifest = package["manifest_path"].as_str().expect("a package has a manifest");
            let directory = std::path::Path::new(manifest).parent().expect("a manifest has a directory");
            directory
                .strip_prefix(&root)
                .expect("a member lives under the repository root")
                .display()
                .to_string()
        })
        .collect();
    assert!(
        packages.len() > unpublished.len(),
        "no member is publishable; the guide's exception would be the rule",
    );

    // Whitespace-normalised, so the sentence may be rewrapped without becoming a false
    // failure. Naming the crate anywhere would not do: the repository map names it too, and a
    // check the map satisfies cannot fail when the exception itself is deleted.
    let guide: String = read("AGENTS.md").split_whitespace().collect::<Vec<_>>().join(" ");
    let stated: BTreeSet<String> = unpublished
        .iter()
        .filter(|directory| guide.contains(&format!("published except `{directory}`")))
        .cloned()
        .collect();
    assert_eq!(
        stated, unpublished,
        "AGENTS.md must state every unpublished member as an exception to what is published",
    );
    assert_eq!(
        guide.matches("published except `").count(),
        unpublished.len(),
        "AGENTS.md states more exceptions than the workspace has",
    );
}

/// Every output a composite action publishes is written by the step it names.
///
/// An action's `value:` is reached by no job output, so the republish rule cannot see it. Left
/// unbound, repointing it at a step output nothing writes still parses, still names a step that
/// exists, and still passes every path check -- and every job in every workflow then receives
/// the empty string.
#[test]
fn every_action_output_is_written_by_the_step_it_names() {
    let mut checked = 0_usize;
    for output in action_outputs() {
        let ActionOutput { source, name, value, steps } = &output;
        let (id, key) = value
            .split_once("steps.")
            .and_then(|(_, tail)| tail.split_once(".outputs."))
            .map(|(id, tail)| (id, tail.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')))
            .unwrap_or_else(|| panic!("{source} publishes {name} as {value}, which reads no step output"));
        let (_, body) = steps
            .iter()
            .find(|(step, _)| step == id)
            .unwrap_or_else(|| panic!("{source} publishes {name} from step {id}, which it has no"));
        let body = body.as_deref().unwrap_or_else(|| {
            panic!(
                "{source} publishes {name} from step {id}, which runs an action rather than a \
                 script; what that step writes is beyond this check"
            )
        });
        // A step writes an output by naming it to $GITHUB_OUTPUT, as an assignment or as the
        // opening of a heredoc. A line merely beginning `<key>=` is a shell variable of that
        // name, which is what this step sets its own delimiter up with. A naming line that
        // redirects somewhere else writes that file instead, whatever the rest of the body
        // mentions; one that redirects nowhere is inside a group, and the group's own redirect
        // is what the body has to carry.
        let writes = body.contains("GITHUB_OUTPUT")
            && body.lines().map(str::trim).any(|line| {
                let names = !line.starts_with(&format!("{key}="))
                    && (line.contains(&format!("{key}<<")) || line.contains(&format!("{key}=")));
                names && (!line.contains('>') || line.contains("GITHUB_OUTPUT"))
            });
        assert!(writes, "{source} publishes {name} from {id}.outputs.{key}, which that step never writes",);
        checked += 1;
    }
    assert!(checked > 0, "no action publishes an output; this would pass vacuously");
}

/// Every step that installs a tool asks for it by the input that can carry a pin.
///
/// The installer answers to a second spelling, `taiki-e/install-action/<tool>@<ref>`, which
/// names the tool in its own path and states no version anywhere: the step pins a commit, the
/// literal scan finds nothing to object to, and the job installs whatever that action resolves
/// on the day it runs. The tool rule cannot see such a step, because it makes no request.
#[test]
fn every_tool_is_requested_by_the_input_that_can_carry_a_pin() {
    let mut steps = 0_usize;
    for (source, scope, clause, requested) in install_action_steps() {
        let action = clause.split_once('@').map_or(clause.as_str(), |(action, _)| action);
        assert_eq!(
            action, INSTALL_ACTION,
            "{source} job {scope} uses {clause}, which names its tool in the path and pins no \
             version; request it through the tool input instead",
        );
        // An empty list is not a request: `tool_requests` drops it, so the pin rule never sees
        // the step and the job installs whatever the runner already had.
        assert!(
            requested.is_some_and(|list| !list.trim().is_empty()),
            "{source} job {scope} uses {clause} and asks for no tool",
        );
        steps += 1;
    }
    assert!(steps > 0, "no step installs a tool; this would pass vacuously");
}

/// The analogue of the toolchain rule, for the tools a job installs. A version literal is a
/// copy of a pin, and the copies are what a bump has to reach. Indexing SOME entry is not
/// enough either: a request reading another tool's entry installs the wrong version under the
/// right name, and the run succeeds.
#[test]
fn every_tool_a_job_installs_reads_its_own_pin() {
    let manifest = tools();
    let mut requested = 0_usize;
    for (source, scope, specification) in tool_requests() {
        let (name, version) = specification
            .split_once('@')
            .unwrap_or_else(|| panic!("{source} job {scope} requests {specification}, which pins nothing"));
        // Two sections defining one tool is refused by `no_tool_is_pinned_twice`, so the
        // reachable failure here is that the manifest pins the tool nowhere.
        let Some((section, _)) = manifest.tool(name).into_iter().next() else {
            panic!(
                "{source} job {scope} installs {name}, which {} pins nowhere; add an entry for it",
                DevTools::PATH,
            )
        };
        let expected = vec![section.to_owned(), name.to_owned(), "version".to_owned()];
        assert_eq!(
            manifest_paths(version),
            vec![expected],
            "{source} job {scope} installs {name} from {version} rather than from its own pin; \
             index it out of the manifest the read-dev-tools action publishes",
        );
        requested += 1;
    }
    assert!(requested > 0, "no tool was requested; this would pass vacuously");
}

/// The tool search order has one home: the `Justfile`'s `export PATH`. Both setup blocks hand a
/// contributor a line for their own shell, and one that prepends inverts the order there while
/// every recipe keeps the other — a machine whose doctor and whose recipes disagree, reported by
/// nothing.
#[test]
fn every_documented_path_export_appends_the_repository_tools() {
    let mut checked = 0_usize;
    for relative in ["Justfile", "README.md", "CONTRIBUTING.md"] {
        let text = read(relative);
        for line in text.lines() {
            if !line.trim_start().starts_with("export PATH") || !line.contains(".tools/bin") {
                continue;
            }
            // A `just` recipe spells the inherited path as a function call; a shell spells it as
            // a variable. An export naming neither replaces PATH rather than extending it.
            let inherited = line
                .find("$PATH")
                .or_else(|| line.find("env_var(\"PATH\")"))
                .unwrap_or_else(|| panic!("{relative} exports a PATH that drops the inherited one: {line}"));
            let local = line.find(".tools/bin").expect("the line was selected for containing it");
            assert!(
                inherited < local,
                "{relative} puts .tools/bin ahead of the inherited PATH, so the repository copy \
                 shadows the host's: {line}",
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no file exported a PATH; this would pass vacuously");
}

/// A published package that carries no source label stands alone in the registry: it inherits no
/// permissions from the repository and nothing points back at what describes it. The image is
/// published so that others can pull it, which is the half a label makes true.
#[test]
fn every_pushed_image_is_labelled_with_the_repository_that_describes_it() {
    let mut pushes = 0_usize;
    for relative in tracked(&[".github/workflows/*.yml"]) {
        let text = read(&relative);
        if !text.contains("docker buildx build --push") {
            continue;
        }
        assert!(
            text.contains("org.opencontainers.image.source="),
            "{relative} pushes an image without labelling its source; the package would be \
             published unlinked from the repository",
        );
        pushes += 1;
    }
    assert!(pushes > 0, "no workflow pushed an image; this would pass vacuously");
}
