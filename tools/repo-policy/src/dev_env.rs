//! The development environment: what `just setup` installs, and what `just doctor` verifies.
//!
//! Versions come from `.config/dev-tools.json` and nowhere else. A pin is a floor unless its
//! entry states why it must be exact, and every row prints the comparison it made, so the two
//! classes never read the same.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

pub const MANIFEST_PATH: &str = ".config/dev-tools.json";
pub const SETUP_HINT: &str = "run just setup";

const DEFAULT_VERSION_ARGS: [&str; 1] = ["--version"];
const TOOL_SECTION_SUFFIX: &str = "_tools";
/// Sections CI installs from and setup does not. Found by prefix rather than listed, so a second
/// CI-only section is a recognised kind rather than one that stops doctor.
const CI_SECTION_PREFIX: &str = "ci_";
const LABEL_WIDTH: usize = 30;

#[derive(Debug, Deserialize)]
pub struct Rust {
    pub primary: String,
    pub msrv: String,
    #[serde(default)]
    pub primary_components: Vec<String>,
    #[serde(default)]
    pub msrv_components: Vec<String>,
    #[serde(default)]
    pub primary_targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub rust: Rust,
    /// Every other top-level key, so a tool section is found by shape rather than by name.
    #[serde(flatten)]
    pub rest: BTreeMap<String, serde_json::Value>,
}

/// One pinned tool, with the defaults an entry may omit.
///
/// The key is the name the tool is requested and installed by, so an entry states `crate`,
/// `package` or `binary` only where one differs from it, and `version_args` only where asking
/// for a version is not `--version`.
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub crate_name: String,
    pub package: String,
    pub binary: String,
    pub version: String,
    pub version_args: Vec<String>,
    /// Why this tool must be the exact version rather than at least it. Absent is a floor.
    pub exact: Option<String>,
}

impl Tool {
    /// Whether a read version answers this tool's pin, by its own class.
    pub fn satisfied_by(&self, installed: Option<&[u64]>) -> bool {
        let (Some(wanted), Some(installed)) = (parse_version(&self.version), installed) else {
            return false;
        };
        if self.exact.is_some() {
            installed == wanted.as_slice()
        } else {
            satisfies_floor(installed, &wanted)
        }
    }
}

pub fn load_manifest(root: &Path) -> Manifest {
    let text = std::fs::read_to_string(root.join(MANIFEST_PATH))
        .unwrap_or_else(|error| panic!("cannot read {MANIFEST_PATH}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{MANIFEST_PATH}: {error}"))
}

/// Every tool section the manifest states, found by shape rather than listed.
pub fn tool_sections(manifest: &Manifest) -> BTreeSet<String> {
    manifest.rest.keys().filter(|key| key.ends_with(TOOL_SECTION_SUFFIX)).cloned().collect()
}

/// Every tool in a section, with the defaults an entry may omit.
///
/// A key nothing reads is refused: `binry` would leave the binary named after the section key,
/// and doctor would report a tool missing that is installed.
pub fn tools(manifest: &Manifest, section: &str) -> Vec<Tool> {
    let entries = manifest
        .rest
        .get(section)
        .unwrap_or_else(|| panic!("{MANIFEST_PATH} states no {section}"))
        .as_object()
        .unwrap_or_else(|| panic!("{MANIFEST_PATH}: {section} is not an object"));

    const KNOWN: [&str; 6] = ["crate", "package", "binary", "version", "version_args", "exact"];
    let mut resolved = Vec::new();
    for (name, entry) in entries {
        let entry =
            entry.as_object().unwrap_or_else(|| panic!("{MANIFEST_PATH}: {section}.{name} is not an object"));
        let unknown: Vec<&String> = entry.keys().filter(|key| !KNOWN.contains(&key.as_str())).collect();
        assert!(
            unknown.is_empty(),
            "{MANIFEST_PATH}: {section}.{name} states {unknown:?}, which nothing reads"
        );
        let text = |key: &str, fallback: &str| -> String {
            entry.get(key).and_then(serde_json::Value::as_str).unwrap_or(fallback).to_owned()
        };
        resolved.push(Tool {
            name: name.clone(),
            crate_name: text("crate", name),
            package: text("package", name),
            binary: text("binary", name),
            version: entry
                .get("version")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("{MANIFEST_PATH}: {section}.{name} states no version"))
                .to_owned(),
            version_args: entry.get("version_args").and_then(serde_json::Value::as_array).map_or_else(
                || DEFAULT_VERSION_ARGS.iter().map(|argument| (*argument).to_owned()).collect(),
                |arguments| {
                    arguments
                        .iter()
                        .map(|argument| argument.as_str().unwrap_or_default().to_owned())
                        .collect()
                },
            ),
            exact: entry.get("exact").and_then(serde_json::Value::as_str).map(str::to_owned),
        });
    }
    resolved
}

/// The one line of a version banner worth displaying.
pub fn first_line(output: &str) -> String {
    output.lines().next().unwrap_or("no output").to_owned()
}

fn is_version_char(character: char) -> bool {
    character.is_ascii_digit() || character == '.'
}

/// Whether output carries one exact dotted version token.
pub fn contains_version(output: &str, version: &str) -> bool {
    let bytes = output.as_bytes();
    let mut from = 0;
    while let Some(offset) = output[from..].find(version) {
        let start = from + offset;
        let end = start + version.len();
        let before = start.checked_sub(1).is_none_or(|index| !is_version_char(char::from(bytes[index])));
        let after = bytes.get(end).is_none_or(|byte| !is_version_char(char::from(*byte)));
        if before && after {
            return true;
        }
        from = start + 1;
    }
    false
}

/// The dotted version in one candidate string, as its numeric parts.
///
/// A maximal run of digits and dots is the only thing that can match: every shorter prefix is
/// followed by a digit or a dot, so the run either is a version or contains none.
fn version_in(candidate: &str) -> Option<Vec<u64>> {
    let characters: Vec<char> = candidate.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        if !is_version_char(characters[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < characters.len() && is_version_char(characters[index]) {
            index += 1;
        }
        let run: String = characters[start..index].iter().collect();
        let parts: Vec<&str> = run.split('.').collect();
        if parts.len() >= 2
            && parts.iter().all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(parts.iter().filter_map(|part| part.parse().ok()).collect());
        }
    }
    None
}

/// Read the dotted version out of a tool's banner.
///
/// The banner line wins and the rest is only a fallback: ShellCheck prints its name on line one
/// and `version: 0.11.0` on line two, while a captured warning carrying a path like
/// `/etc/foo/1.2.3/` would otherwise be read as the version.
pub fn parse_version(output: &str) -> Option<Vec<u64>> {
    version_in(&first_line(output)).or_else(|| version_in(output))
}

/// Whether an installed version is at least the pinned one.
///
/// Compared as integers, never as text: `0.9.140` is above `0.9.9` by number and below it by
/// string order.
pub fn satisfies_floor(installed: &[u64], floor: &[u64]) -> bool {
    let width = installed.len().max(floor.len());
    let pad = |parts: &[u64]| -> Vec<u64> {
        let mut padded = parts.to_vec();
        padded.resize(width, 0);
        padded
    };
    pad(installed) >= pad(floor)
}

/// The directories `just setup` installs into, in the order the Justfile appends them.
pub fn search_suffixes(root: &Path) -> Vec<PathBuf> {
    vec![root.join(".tools/bin"), root.join("node_modules/.bin")]
}

/// The tool search path the Justfile exports for every recipe.
///
/// Empty entries are dropped: a lookup reads one as the working directory, so an unset PATH would
/// let a file in the repository root answer for a pinned tool — and doctor runs what it finds.
pub fn search_path(root: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> =
        std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect()).unwrap_or_default();
    entries.extend(search_suffixes(root));
    entries.into_iter().filter(|entry| !entry.as_os_str().is_empty()).collect()
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|data| data.is_file() && data.permissions().mode() & 0o111 != 0)
}

/// Find one tool the way a recipe does: the host's PATH, then repository-local.
pub fn resolve(root: &Path, binary: &str) -> Option<PathBuf> {
    search_path(root).into_iter().map(|directory| directory.join(binary)).find(|path| is_executable(path))
}

/// Find one tool the way anything outside this repository would: the host's PATH alone.
pub fn resolve_system(binary: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|directory| directory.join(binary))
        .find(|path| is_executable(path))
}

/// Whether a tool is the copy setup installs.
///
/// The directory is resolved and the file is not: a lookup returns whichever spelling PATH
/// carried, while resolving the file would follow a binary that is a symlink into a store
/// elsewhere, out of the directory that defines it.
pub fn is_repository_local(root: &Path, path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(parent) = parent.canonicalize() else {
        return false;
    };
    search_suffixes(root).into_iter().any(|suffix| suffix.canonicalize().is_ok_and(|suffix| suffix == parent))
}

/// Run one diagnostic command and combine its output streams.
pub fn capture(root: &Path, command: &Path, arguments: &[String]) -> (i32, String) {
    let Ok(output) = Command::new(command).args(arguments).current_dir(root).output() else {
        // The error text is an absolute host path and no remedy; callers describe the absence
        // themselves, in words a reader can act on.
        return (127, "not executable".to_owned());
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.trim().is_empty() {
        stderr.into_owned()
    } else if stderr.trim().is_empty() {
        stdout.into_owned()
    } else {
        format!("{stdout}{stderr}")
    };
    (output.status.code().unwrap_or(127), combined.trim().to_owned())
}

/// Read a Node tool's version from the `package.json` beside its binary.
///
/// Asking the binary is not an option: markdownlint-cli2 treats every argument as a glob, so a
/// version query lints the whole repository.
///
/// The repository's own package is read by path only for a repository-local binary, where
/// `npm ci --no-bin-links` and filesystems without symlinks leave `.bin` entries that lead
/// nowhere. Offering it to a host binary too would let a host copy be judged by the version the
/// repository pinned, and pass.
pub fn node_package_version(root: &Path, binary: &Path, package: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if is_repository_local(root, binary) {
        candidates.push(root.join("node_modules").join(package));
    }
    let resolved = binary.canonicalize().unwrap_or_else(|_| binary.to_path_buf());
    candidates.extend(resolved.ancestors().map(Path::to_path_buf));

    for directory in candidates {
        let manifest = directory.join("package.json");
        if !manifest.is_file() {
            continue;
        }
        // An unreadable manifest between here and the owning package is not the answer; keep
        // looking rather than abandoning the walk.
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if data.get("name").and_then(serde_json::Value::as_str) == Some(package) {
            return data.get("version").and_then(serde_json::Value::as_str).map(str::to_owned);
        }
    }
    None
}

/// ANSI styling that disappears for a pipe, a file, or `NO_COLOR`.
#[derive(Clone, Copy)]
pub struct Palette {
    enabled: bool,
}

impl Palette {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Enabled only for an interactive stdout without `NO_COLOR` set.
    pub fn for_stdout() -> Self {
        use std::io::IsTerminal;
        Self::new(std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
    }

    pub fn paint(self, text: &str, styles: &[&str]) -> String {
        if !self.enabled || text.is_empty() {
            return text.to_owned();
        }
        let opening: String = styles
            .iter()
            .map(|style| match *style {
                "bold" => "\x1b[1m",
                "dim" => "\x1b[2m",
                "red" => "\x1b[31m",
                "green" => "\x1b[32m",
                "yellow" => "\x1b[33m",
                "cyan" => "\x1b[36m",
                other => panic!("unknown style {other}"),
            })
            .collect();
        format!("{opening}{text}\x1b[0m")
    }
}

/// Accumulated gate-prerequisite diagnostics.
pub struct Doctor {
    style: Palette,
    passes: usize,
    system: usize,
    failed: Vec<String>,
    rows: Vec<String>,
}

impl Doctor {
    pub fn new(style: Palette) -> Self {
        Self { style, passes: 0, system: 0, failed: Vec::new(), rows: Vec::new() }
    }

    /// Every row printed so far, so a check's rendering is assertable without reading stdout.
    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    pub fn failed(&self) -> &[String] {
        &self.failed
    }

    pub const LABEL_WIDTH: usize = LABEL_WIDTH;

    pub fn section(&self, title: &str) {
        println!("\n{}", self.style.paint(title, &["bold", "cyan"]));
    }

    fn row(&mut self, marker: &str, label: &str, detail: &str) {
        let rendered = format!("  {marker} {label:<LABEL_WIDTH$} {detail}");
        println!("{rendered}");
        self.rows.push(rendered);
    }

    pub fn ok(&mut self, label: &str, detail: &str) {
        let (marker, detail) = (self.style.paint("✓", &["green"]), self.style.paint(detail, &["dim"]));
        self.row(&marker, label, &detail);
        self.passes += 1;
    }

    /// A pinned tool served from outside the repository.
    pub fn ok_system(&mut self, label: &str, detail: &str, path: &Path) {
        let detail = format!(
            "{}  {}",
            self.style.paint(detail, &["dim"]),
            self.style.paint(&format!("(system: {})", path.display()), &["dim"])
        );
        let marker = self.style.paint("•", &["yellow"]);
        self.row(&marker, label, &detail);
        self.passes += 1;
        self.system += 1;
    }

    pub fn fail(&mut self, label: &str, detail: &str, hint: &str) {
        let marker = self.style.paint("✗", &["red"]);
        let detail = self.style.paint(&format!("{detail} — {hint}"), &["red"]);
        self.row(&marker, label, &detail);
        self.failed.push(label.to_owned());
    }

    /// A check whose only outcome is present or absent.
    pub fn verdict(&mut self, label: &str, passed: bool, detail: &str, hint: &str) {
        if passed {
            self.ok(label, detail);
        } else {
            self.fail(label, detail, hint);
        }
    }

    pub fn summary(&self) -> i32 {
        let counts = format!(
            "{}   {}",
            self.style.paint(&format!("✓ {} ok", self.passes), &["green"]),
            self.style.paint(&format!("• {} system", self.system), &["yellow"])
        );
        println!();
        if self.failed.is_empty() {
            println!("{}   {counts}", self.style.paint("✓ all good", &["bold", "green"]));
            return 0;
        }
        println!(
            "{}   {counts}",
            self.style.paint(&format!("✗ {} failed", self.failed.len()), &["bold", "red"])
        );
        println!("{}", self.style.paint(&format!("  fix: {}", self.failed.join(", ")), &["red"]));
        1
    }
}

/// Judge one tool against its pin and record the verdict.
///
/// A version that cannot be read is reported as unreadable, never as satisfied: a check that
/// cannot see its input has not passed it.
#[allow(clippy::too_many_arguments)]
pub fn check_version(
    root: &Path,
    checks: &mut Doctor,
    label: &str,
    detail: &str,
    installed: Option<&[u64]>,
    pinned: &str,
    hint: &str,
    exact: Option<&str>,
    path: Option<&Path>,
) {
    let Some(wanted) = parse_version(pinned) else {
        checks.fail(label, detail, &format!("unreadable pin {pinned} in the manifest"));
        return;
    };
    let Some(installed) = installed else {
        checks.fail(label, detail, &format!("no readable version; {hint}"));
        return;
    };

    // The host answers before anything setup installs, so a host copy that misses its pin cannot
    // be fixed by installing another one. Naming setup here would send the reader to a command
    // that refuses this exact case.
    let hint = match path {
        Some(path) if !is_repository_local(root, path) => format!("upgrade or remove {}", path.display()),
        _ => hint.to_owned(),
    };

    let (satisfied, comparison, remedy) = if let Some(reason) = exact {
        (
            installed == wanted.as_slice(),
            format!("= {pinned}"),
            format!("is not the pinned {pinned} ({reason}); {hint}"),
        )
    } else {
        (
            satisfies_floor(installed, &wanted),
            format!("≥ {pinned}"),
            format!("below the pinned {pinned}; {hint}"),
        )
    };

    if !satisfied {
        checks.fail(label, detail, &remedy);
        return;
    }

    // ShellCheck names itself on line one and versions itself on line two, so the banner alone
    // does not always show what was compared.
    let found = installed.iter().map(u64::to_string).collect::<Vec<_>>().join(".");
    let shown = if detail.contains(&found) { detail.to_owned() } else { format!("{detail} {found}") };
    match path {
        Some(path) if !is_repository_local(root, path) => {
            checks.ok_system(label, &format!("{shown}  {comparison}"), path);
        }
        _ => checks.ok(label, &format!("{shown}  {comparison}")),
    }
}

fn check_cargo_tool(root: &Path, checks: &mut Doctor, tool: &Tool) {
    let Some(path) = resolve(root, &tool.binary) else {
        checks.fail(&tool.binary, "not found", SETUP_HINT);
        return;
    };
    let (status, output) = capture(root, &path, &tool.version_args);
    let installed = if status == 0 { parse_version(&output) } else { None };
    check_version(
        root,
        checks,
        &tool.binary,
        &first_line(&output),
        installed.as_deref(),
        &tool.version,
        SETUP_HINT,
        tool.exact.as_deref(),
        Some(&path),
    );
}

fn check_node_tool(root: &Path, checks: &mut Doctor, tool: &Tool) {
    let Some(path) = resolve(root, &tool.binary) else {
        checks.fail(&tool.binary, "not found", SETUP_HINT);
        return;
    };
    let Some(actual) = node_package_version(root, &path, &tool.package) else {
        checks.fail(
            &tool.binary,
            "version unreadable",
            &format!("no {} package.json beside it; {SETUP_HINT}", tool.package),
        );
        return;
    };
    check_version(
        root,
        checks,
        &tool.binary,
        &format!("{} {actual}", tool.binary),
        parse_version(&actual).as_deref(),
        &tool.version,
        SETUP_HINT,
        tool.exact.as_deref(),
        Some(&path),
    );
}

/// What to do about a tool `just setup` cannot install.
///
/// It names the version because setup will not supply one, and it is built from the entry so
/// that version is not also written beside the pin.
fn system_install_hint(tool: &Tool) -> String {
    format!(
        "install {} {} with the operating system package manager, then rerun setup",
        tool.package, tool.version
    )
}

fn check_system_tool(root: &Path, checks: &mut Doctor, tool: &Tool) {
    let found = resolve_system(&tool.binary);
    check_system_tool_at(root, checks, tool, found.as_deref());
}

/// Check one tool the operating system package manager owns, wherever it was found.
pub fn check_system_tool_at(root: &Path, checks: &mut Doctor, tool: &Tool, path: Option<&Path>) {
    let hint = system_install_hint(tool);
    let Some(path) = path else {
        checks.fail(&tool.binary, "not found", &hint);
        return;
    };
    let (status, output) = capture(root, path, &tool.version_args);
    let installed = if status == 0 { parse_version(&output) } else { None };
    // The path is carried in the detail rather than handed to `check_version`, which reads one as
    // a copy shadowing the repository's and answers with a remedy setup owns. Nothing in this
    // section is setup's, so the remedy stays the package manager's.
    check_version(
        root,
        checks,
        &tool.binary,
        &format!("{} ({})", first_line(&output), path.display()),
        installed.as_deref(),
        &tool.version,
        &hint,
        tool.exact.as_deref(),
        None,
    );
}

/// Which checker owns each tool section, and the banner it reports under.
type SectionCheck = (&'static str, &'static str, fn(&Path, &mut Doctor, &Tool));

fn section_checks() -> Vec<SectionCheck> {
    vec![
        ("cargo_tools", "Repository tools", check_cargo_tool as fn(&Path, &mut Doctor, &Tool)),
        ("node_tools", "Repository tools", check_node_tool),
        ("system_tools", "System tools", check_system_tool),
    ]
}

fn check_bootstrap(root: &Path, checks: &mut Doctor, command: &str, version: Option<&str>) {
    let Some(path) = resolve_system(command) else {
        checks.fail(command, "not found", "install it before setup");
        return;
    };
    let (status, output) = capture(root, &path, &["--version".to_owned()]);
    if status != 0 {
        // It ran and refused. Naming a version it never printed would send the reader after the
        // wrong thing.
        checks.fail(command, &first_line(&output), &format!("exited {status}"));
        return;
    }
    checks.verdict(
        command,
        version.is_none_or(|version| contains_version(&output, version)),
        &first_line(&output),
        &version.map_or_else(|| "no version reported".to_owned(), |v| format!("expected {v}")),
    );
}

fn installed_rust_items(root: &Path, toolchain: &str, item: &str) -> (bool, BTreeSet<String>) {
    let Some(rustup) = resolve_system("rustup") else {
        return (false, BTreeSet::new());
    };
    let arguments: Vec<String> =
        [item, "list", "--toolchain", toolchain, "--installed"].iter().map(|a| (*a).to_owned()).collect();
    let (status, output) = capture(root, &rustup, &arguments);
    (status == 0, output.lines().map(str::to_owned).collect())
}

/// Match rustup's target-qualified component names.
fn component_present(installed: &BTreeSet<String>, component: &str) -> bool {
    installed.contains(component) || installed.iter().any(|item| item.starts_with(&format!("{component}-")))
}

/// Describe one rustup component or target so the row matches its marker.
///
/// An absent item reads `missing`. Reporting it as `installed` beside a failing marker is how a
/// missing `rust-src` gets mistaken for a present one, and the compile-fail goldens re-blessed
/// against it.
fn rust_item_detail(available: bool, present: bool) -> &'static str {
    if !available {
        "toolchain unavailable"
    } else if present {
        "installed"
    } else {
        "missing"
    }
}

fn check_toolchain(root: &Path, checks: &mut Doctor, label: &str, version: &str, components: &[String]) {
    let rustup = resolve_system("rustup");
    let (status, output) = rustup.as_ref().map_or_else(
        || (127, "not executable".to_owned()),
        |rustup| {
            let arguments: Vec<String> =
                ["run", version, "rustc", "--version"].iter().map(|a| (*a).to_owned()).collect();
            capture(root, rustup, &arguments)
        },
    );
    checks.verdict(
        label,
        status == 0 && contains_version(&output, version),
        &first_line(&output),
        &format!("expected {version}; {SETUP_HINT}"),
    );

    let (available, installed) = installed_rust_items(root, version, "component");
    for component in components {
        let present = available && component_present(&installed, component);
        checks.verdict(
            &format!("{version} {component}"),
            present,
            rust_item_detail(available, present),
            &format!("rustup component add {component} --toolchain {version}"),
        );
    }
}

fn check_targets(root: &Path, checks: &mut Doctor, version: &str, targets: &[String]) {
    let (available, installed) = installed_rust_items(root, version, "target");
    for target in targets {
        let present = available && installed.contains(target);
        checks.verdict(
            &format!("{version} {target}"),
            present,
            rust_item_detail(available, present),
            &format!("rustup target add {target} --toolchain {version}"),
        );
    }
}

/// Verify every prerequisite reached by the root gate.
///
/// Every tool section the manifest states is found by shape and dispatched here, and the two have
/// to agree in both directions: a section nothing checks reads exactly like a section that
/// passed. Both are settled before the first row is printed, so a manifest doctor cannot report
/// on stops instead of half-reporting.
pub fn doctor(root: &Path, manifest: &Manifest) -> i32 {
    let checkers = section_checks();
    let owned: BTreeSet<String> = checkers.iter().map(|(section, _, _)| (*section).to_owned()).collect();
    let stated = tool_sections(manifest);

    let unknown: Vec<&String> =
        stated.difference(&owned).filter(|section| !section.starts_with(CI_SECTION_PREFIX)).collect();
    assert!(unknown.is_empty(), "{MANIFEST_PATH} states tool sections nothing checks: {unknown:?}");
    let missing: Vec<&String> = owned.difference(&stated).collect();
    assert!(missing.is_empty(), "{MANIFEST_PATH} states no {missing:?}, which doctor checks");

    let style = Palette::for_stdout();
    let mut checks = Doctor::new(style);
    println!(
        "\n{}  {}",
        style.paint("kamu · doctor", &["bold", "cyan"]),
        style.paint("root-gate prerequisites", &["dim"])
    );

    checks.section("Bootstrap commands");
    let primary = manifest.rust.primary.clone();
    for (command, version) in [
        ("git", None),
        ("rustup", None),
        ("cargo", Some(primary.as_str())),
        ("rustc", Some(primary.as_str())),
        ("python3", None),
        ("node", None),
        ("npm", None),
    ] {
        check_bootstrap(root, &mut checks, command, version);
    }

    checks.section("Rust toolchains and components");
    check_toolchain(
        root,
        &mut checks,
        "primary compiler",
        &manifest.rust.primary,
        &manifest.rust.primary_components,
    );
    check_toolchain(root, &mut checks, "MSRV compiler", &manifest.rust.msrv, &manifest.rust.msrv_components);
    check_targets(root, &mut checks, &manifest.rust.primary, &manifest.rust.primary_targets);

    let mut banners: Vec<&str> = Vec::new();
    for (_, banner, _) in &checkers {
        if !banners.contains(banner) {
            banners.push(banner);
        }
    }
    for banner in banners {
        checks.section(banner);
        for (section, section_banner, check) in &checkers {
            if section_banner != &banner {
                continue;
            }
            for tool in tools(manifest, section) {
                check(root, &mut checks, &tool);
            }
        }
    }

    checks.section("Vendored data");
    let submodule = root.join("crates/iso3166/vendor/iso3166-csv/countries.csv");
    checks.verdict(
        "ISO 3166 submodule",
        submodule.is_file(),
        "initialized",
        &format!("not initialized; {SETUP_HINT}"),
    );

    checks.summary()
}

fn render(command: &[String]) -> String {
    command.join(" ")
}

fn run(root: &Path, command: &[String]) {
    println!("+ {}", render(command));
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(root)
        .status()
        .unwrap_or_else(|error| panic!("cannot run {}: {error}", command[0]));
    assert!(status.success(), "{} failed: {status}", render(command));
}

/// The deterministic non-Cargo portion of the setup command list.
pub fn setup_commands(manifest: &Manifest) -> Vec<Vec<String>> {
    let rust = &manifest.rust;
    let owned = |parts: &[&str]| -> Vec<String> { parts.iter().map(|p| (*p).to_owned()).collect() };
    let mut commands = vec![owned(&["git", "submodule", "update", "--init", "--recursive"])];

    for (version, components) in
        [(&rust.primary, &rust.primary_components), (&rust.msrv, &rust.msrv_components)]
    {
        let mut command = owned(&["rustup", "toolchain", "install", version, "--profile", "minimal"]);
        for component in components {
            command.push("--component".to_owned());
            command.push(component.clone());
        }
        commands.push(command);
    }

    let mut targets = owned(&["rustup", "target", "add", "--toolchain", &rust.primary]);
    targets.extend(rust.primary_targets.iter().cloned());
    commands.push(targets);
    commands.push(owned(&["npm", "ci", "--no-fund", "--no-audit"]));
    commands
}

/// One exact repository-local Cargo install command.
pub fn cargo_install_command(root: &Path, primary: &str, tool: &Tool) -> Vec<String> {
    vec![
        "rustup".to_owned(),
        "run".to_owned(),
        primary.to_owned(),
        "cargo".to_owned(),
        "install".to_owned(),
        "--locked".to_owned(),
        "--force".to_owned(),
        "--root".to_owned(),
        root.join(".tools").display().to_string(),
        "--version".to_owned(),
        format!("={}", tool.version),
        tool.crate_name.clone(),
    ]
}

/// Install exact toolchains, targets and repository-local tools, then verify them.
pub fn setup(root: &Path, manifest: &Manifest) -> i32 {
    let missing: Vec<&str> = ["git", "rustup", "cargo", "node", "npm"]
        .into_iter()
        .filter(|command| resolve_system(command).is_none())
        .collect();
    if !missing.is_empty() {
        eprintln!("setup: install bootstrap command(s) first: {}", missing.join(", "));
        return 1;
    }

    let commands = setup_commands(manifest);
    let (npm, rest) = commands.split_last().expect("setup runs at least one command");
    for command in rest {
        run(root, command);
    }

    std::fs::create_dir_all(root.join(".tools")).expect(".tools is creatable");
    let primary = &manifest.rust.primary;
    let mut shadowed: Vec<(Tool, PathBuf)> = Vec::new();

    for tool in tools(manifest, "cargo_tools") {
        let found = resolve(root, &tool.binary);
        let installed = found.as_ref().and_then(|path| {
            let (status, output) = capture(root, path, &tool.version_args);
            if status == 0 { parse_version(&output) } else { None }
        });
        if tool.satisfied_by(installed.as_deref()) {
            let where_ = if found.as_ref().is_some_and(|path| is_repository_local(root, path)) {
                "repository-local"
            } else {
                "on this host"
            };
            println!("= {} already {where_}", tool.binary);
            continue;
        }
        // The host answers before the repository-local copy, so installing beneath an
        // unsatisfying host copy leaves a binary no recipe will ever run.
        if let Some(path) = found.filter(|path| !is_repository_local(root, path)) {
            shadowed.push((tool, path));
            continue;
        }
        let command = cargo_install_command(root, primary, &tool);
        run(root, &command);
    }

    run(root, npm);

    // npm installs the whole tree in one command, so a shadowed Node tool cannot be skipped the
    // way a Cargo one is. It is reported for the same reason: the install below
    // `node_modules/.bin` succeeds and no recipe will ever reach it.
    for tool in tools(manifest, "node_tools") {
        let Some(found) = resolve(root, &tool.binary) else {
            continue;
        };
        if is_repository_local(root, &found) {
            continue;
        }
        let actual = node_package_version(root, &found, &tool.package);
        let installed = actual.as_deref().and_then(parse_version);
        if !tool.satisfied_by(installed.as_deref()) {
            shadowed.push((tool, found));
        }
    }

    for (tool, found) in &shadowed {
        eprintln!(
            "! {} on this host does not answer its pin, and it comes before the repository-local \
             copy; upgrade or remove {}",
            tool.binary,
            found.display()
        );
    }

    doctor(root, manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_from_the_banner_line_first() {
        assert_eq!(Some(vec![1, 2, 3]), parse_version("tool 1.2.3"));
        // ShellCheck names itself on line one and versions itself below.
        assert_eq!(Some(vec![0, 11, 0]), parse_version("ShellCheck\nversion: 0.11.0"));
    }

    #[test]
    fn a_path_below_the_banner_is_not_mistaken_for_the_version() {
        assert_eq!(Some(vec![1, 2, 3]), parse_version("tool 1.2.3\nwarning: /etc/foo/9.9.9/bar"));
    }

    #[test]
    fn a_build_date_is_not_a_version() {
        assert_eq!(None, parse_version("built 20260818"));
        assert_eq!(None, parse_version("no version here"));
    }

    #[test]
    fn the_floor_comparison_is_numeric_not_lexical() {
        assert!(satisfies_floor(&[0, 9, 140], &[0, 9, 9]));
        assert!(!satisfies_floor(&[0, 9, 9], &[0, 9, 140]));
        assert!(satisfies_floor(&[1, 2, 3], &[1, 2, 3]));
        assert!(!satisfies_floor(&[1, 2, 2], &[1, 2, 3]));
    }

    #[test]
    fn a_shorter_version_is_padded_not_truncated() {
        assert!(satisfies_floor(&[1, 2], &[1, 2, 0]));
        assert!(!satisfies_floor(&[1, 2], &[1, 2, 1]));
    }

    #[test]
    fn a_version_token_must_stand_alone() {
        assert!(contains_version("rustc 1.97.1", "1.97.1"));
        assert!(!contains_version("rustc 1.97.10", "1.97.1"));
        assert!(!contains_version("rustc 11.97.1", "1.97.1"));
    }

    #[test]
    fn a_disabled_palette_emits_no_escape_codes() {
        assert_eq!("plain", Palette::new(false).paint("plain", &["red", "bold"]));
    }

    #[test]
    fn an_enabled_palette_wraps_and_resets() {
        assert_eq!("\x1b[31mred\x1b[0m", Palette::new(true).paint("red", &["red"]));
        assert_eq!("", Palette::new(true).paint("", &["red"]));
    }

    fn doctor() -> Doctor {
        Doctor::new(Palette::new(false))
    }

    #[test]
    fn a_clean_run_exits_zero() {
        let mut checks = doctor();
        checks.ok("tool", "1.2.3");
        assert_eq!(0, checks.summary());
        assert!(checks.failed().is_empty());
    }

    #[test]
    fn a_failure_exits_one_and_lists_what_to_fix() {
        let mut checks = doctor();
        checks.fail("tool", "not found", "run just setup");
        assert_eq!(1, checks.summary());
        assert_eq!(["tool"], checks.failed());
    }

    #[test]
    fn a_failing_row_carries_its_remedy() {
        let mut checks = doctor();
        checks.fail("tool", "not found", "run just setup");
        assert!(checks.rows()[0].contains("not found — run just setup"));
    }

    #[test]
    fn a_system_copy_passes_and_is_counted_apart() {
        let mut checks = doctor();
        checks.ok_system("tool", "1.2.3", Path::new("/opt/bin/tool"));
        assert!(checks.failed().is_empty());
        assert!(checks.rows()[0].contains("(system: /opt/bin/tool)"));
        assert_eq!(0, checks.summary());
    }

    /// A pin is a floor unless the entry states why it must be exact, and the row prints the
    /// comparison it made so the two classes never read the same.
    #[test]
    fn a_floor_and_an_exact_pin_print_different_comparisons() {
        let root = crate::repo_root();

        let mut checks = doctor();
        check_version(
            &root,
            &mut checks,
            "tool",
            "tool 1.2.4",
            Some(&[1, 2, 4]),
            "1.2.3",
            "hint",
            None,
            None,
        );
        assert!(checks.rows()[0].contains("≥ 1.2.3"), "{:?}", checks.rows());
        assert!(checks.failed().is_empty());

        let mut checks = doctor();
        check_version(
            &root,
            &mut checks,
            "tool",
            "tool 1.2.4",
            Some(&[1, 2, 4]),
            "1.2.3",
            "hint",
            Some("the output is the verdict"),
            None,
        );
        assert_eq!(["tool"], checks.failed(), "an exact pin refuses a newer copy");
        assert!(
            checks.rows()[0].contains("is not the pinned 1.2.3 (the output is the verdict)"),
            "{:?}",
            checks.rows()
        );

        let mut checks = doctor();
        check_version(
            &root,
            &mut checks,
            "tool",
            "tool 1.2.3",
            Some(&[1, 2, 3]),
            "1.2.3",
            "hint",
            Some("the output is the verdict"),
            None,
        );
        assert!(checks.failed().is_empty());
        assert!(checks.rows()[0].contains("= 1.2.3"), "an exact pin shows equality, not a floor");
    }

    #[test]
    fn an_unreadable_version_fails_rather_than_passing() {
        let root = crate::repo_root();
        let mut checks = doctor();
        check_version(&root, &mut checks, "tool", "no output", None, "1.2.3", "hint", None, None);
        assert_eq!(["tool"], checks.failed());
        assert!(checks.rows()[0].contains("no readable version"));
    }

    #[test]
    fn an_unreadable_pin_is_reported_against_the_manifest() {
        let root = crate::repo_root();
        let mut checks = doctor();
        check_version(
            &root,
            &mut checks,
            "tool",
            "tool 1.2.3",
            Some(&[1, 2, 3]),
            "latest",
            "hint",
            None,
            None,
        );
        assert_eq!(["tool"], checks.failed());
        assert!(checks.rows()[0].contains("unreadable pin latest"));
    }

    #[test]
    fn a_version_absent_from_the_banner_is_still_shown() {
        let root = crate::repo_root();
        let mut checks = doctor();
        check_version(
            &root,
            &mut checks,
            "shellcheck",
            "ShellCheck",
            Some(&[0, 11, 0]),
            "0.10",
            "hint",
            None,
            None,
        );
        assert!(checks.rows()[0].contains("ShellCheck 0.11.0"), "{:?}", checks.rows());
    }

    /// A shadowing host copy cannot be fixed by installing another one, so the remedy is not setup.
    #[test]
    fn a_shadowing_host_copy_is_told_to_move_rather_than_to_run_setup() {
        let root = crate::repo_root();
        let shadow = Path::new("/opt/bin/tool");
        let mut checks = doctor();
        check_version(
            &root,
            &mut checks,
            "tool",
            "tool 1.0.0",
            Some(&[1, 0, 0]),
            "2.0.0",
            SETUP_HINT,
            None,
            Some(shadow),
        );
        let row = &checks.rows()[0];
        assert!(row.contains("upgrade or remove /opt/bin/tool"), "{row}");
        assert!(!row.contains(SETUP_HINT), "{row}");
    }

    #[test]
    fn an_absent_component_reads_missing_not_installed() {
        assert_eq!("missing", rust_item_detail(true, false));
        assert_eq!("installed", rust_item_detail(true, true));
        assert_eq!("toolchain unavailable", rust_item_detail(false, false));
    }

    #[test]
    fn a_component_matches_its_target_qualified_name() {
        let installed: BTreeSet<String> =
            ["rust-src", "llvm-tools-x86_64-unknown-linux-gnu"].iter().map(|s| (*s).to_owned()).collect();
        assert!(component_present(&installed, "rust-src"));
        assert!(component_present(&installed, "llvm-tools"));
        assert!(!component_present(&installed, "miri"));
    }

    #[test]
    fn an_absent_tool_resolves_to_nothing() {
        assert_eq!(None, resolve(&crate::repo_root(), "kamu-nonexistent-binary-probe"));
    }
}
