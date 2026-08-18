//! What `just setup` installs and `just doctor` reports, held to `.config/dev-tools.json`.

use std::collections::BTreeSet;

use repo_policy::dev_env::{
    Doctor, Palette, cargo_install_command, load_manifest, setup_commands, tool_sections, tools,
};
use repo_policy::{read, repo_root};

/// The extension lane installs its own channel. Setup does not, because a developer who never
/// enters the lane would download a second toolchain for nothing. Stated rather than derived, so
/// a channel added without a setup command still fails.
const INSTALLED_BY_THE_LANE: [&str; 1] = ["lane"];

/// Every dotted version literal in a file, found where the given prefix introduces one.
fn literals_after(text: &str, prefixes: &[&str]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for line in text.lines() {
        for prefix in prefixes {
            let mut from = 0;
            while let Some(offset) = line[from..].find(prefix) {
                let start = from + offset + prefix.len();
                let literal: String =
                    line[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                let literal = literal.trim_end_matches('.').to_owned();
                if literal.contains('.') {
                    found.insert(literal);
                }
                from = start.max(from + offset + 1);
            }
        }
    }
    found
}

/// `rustup run <version>` addresses a toolchain by name, so a literal that outlives a bump either
/// resolves to a stale install or fails to resolve at all. Both read as the gate proving
/// something CI does not.
#[test]
fn every_pinned_toolchain_literal_names_a_manifest_version() {
    let manifest = load_manifest(&repo_root());
    let mut allowed = BTreeSet::new();
    for version in [&manifest.rust.msrv, &manifest.rust.primary] {
        allowed.insert(version.clone());
        let series: Vec<&str> = version.split('.').take(2).collect();
        allowed.insert(series.join("."));
    }

    for (file, prefixes) in [
        ("Justfile", vec!["cargo +", "rustup run ", "msrv("]),
        ("README.md", vec!["Rust-", "Rust "]),
        // Clippy gates which lints apply on this, so a stale value lints the workspace against a
        // Rust it no longer supports.
        ("clippy.toml", vec!["msrv = \""]),
    ] {
        let found = literals_after(&read(file), &prefixes);
        assert!(!found.is_empty(), "{file}: no pinned toolchain literal to bind");
        let stray: Vec<&String> = found.difference(&allowed).collect();
        assert!(stray.is_empty(), "{file} names {stray:?}, which the manifest does not");
    }

    let labels = literals_after(&read("Justfile"), &["msrv("]);
    assert!(!labels.is_empty(), "no msrv stage label to bind");
    for label in &labels {
        assert_eq!(&manifest.rust.msrv, label, "the msrv stage label names another version");
    }
}

#[test]
fn setup_commands_install_every_required_rust_item() {
    let root = repo_root();
    let manifest = load_manifest(&root);
    let commands = setup_commands(&manifest);
    let rendered: Vec<String> = commands.iter().map(|command| command.join(" ")).collect();

    // A channel is a key naming a version; the component and target lists are keyed off one.
    let channels: BTreeSet<&str> =
        [manifest.rust.primary.as_str(), manifest.rust.msrv.as_str()].into_iter().collect();
    assert!(!channels.is_empty(), "the manifest names no channel to install");

    for (channel, version, components) in [
        ("primary", &manifest.rust.primary, &manifest.rust.primary_components),
        ("msrv", &manifest.rust.msrv, &manifest.rust.msrv_components),
    ] {
        if INSTALLED_BY_THE_LANE.contains(&channel) {
            continue;
        }
        assert!(!components.is_empty(), "{channel} lists no component");
        let installs: Vec<&String> = rendered
            .iter()
            .filter(|command| command.contains(&format!("toolchain install {version}")))
            .collect();
        // Exactly one, so two channels sharing a version cannot satisfy each other's components
        // through a command built for the other.
        assert_eq!(1, installs.len(), "setup builds {} install commands for {channel}", installs.len());
        for component in components {
            assert!(
                installs[0].contains(&format!("--component {component}")),
                "setup installs {channel} without {component}"
            );
        }
    }

    assert!(
        commands.iter().any(|command| {
            command.len() >= 5
                && command[..5] == ["rustup", "target", "add", "--toolchain", manifest.rust.primary.as_str()]
        }),
        "setup adds no targets to the primary toolchain"
    );
    assert!(
        commands.iter().any(|command| command == &["npm", "ci", "--no-fund", "--no-audit"]),
        "setup does not install the Node tools from the lockfile"
    );
    assert!(
        !commands.iter().flatten().any(|argument| argument.contains("||")),
        "a setup command shells out rather than running a program"
    );
}

#[test]
fn every_cargo_tool_install_is_locked_and_exact() {
    let root = repo_root();
    let manifest = load_manifest(&root);
    for tool in tools(&manifest, "cargo_tools") {
        let command = cargo_install_command(&root, &manifest.rust.primary, &tool);
        assert!(command.contains(&"--locked".to_owned()), "{}: not locked", tool.crate_name);
        assert!(command.contains(&"--force".to_owned()), "{}: not forced", tool.crate_name);
        let index = command.iter().position(|argument| argument == "--version").expect("names a version");
        assert_eq!(format!("={}", tool.version), command[index + 1], "{}", tool.crate_name);
    }
}

#[test]
fn the_node_manifest_and_lock_use_the_exact_pin() {
    let manifest = load_manifest(&repo_root());
    let package: serde_json::Value =
        serde_json::from_str(&read("package.json")).expect("package.json parses");
    let lock: serde_json::Value =
        serde_json::from_str(&read("package-lock.json")).expect("package-lock.json parses");

    let mut checked = 0_usize;
    for tool in tools(&manifest, "node_tools") {
        assert_eq!(
            Some(tool.version.as_str()),
            package["devDependencies"][&tool.package].as_str(),
            "package.json pins {} elsewhere",
            tool.package
        );
        assert_eq!(
            Some(tool.version.as_str()),
            lock["packages"][""]["devDependencies"][&tool.package].as_str(),
            "package-lock.json pins {} elsewhere",
            tool.package
        );
        checked += 1;
    }
    assert!(checked > 0, "the manifest pins no Node tool to check");
}

/// A label too long for the column breaks alignment rather than any check, so the width is held
/// against every label doctor can print.
#[test]
fn every_manifest_label_fits_the_reported_column() {
    let manifest = load_manifest(&repo_root());
    let rust = &manifest.rust;
    let mut labels: Vec<String> = rust
        .primary_components
        .iter()
        .chain(rust.primary_targets.iter())
        .map(|item| format!("{} {item}", rust.primary))
        .collect();
    labels.extend(rust.msrv_components.iter().map(|item| format!("{} {item}", rust.msrv)));

    for section in tool_sections(&manifest) {
        if section.starts_with("ci_") {
            continue;
        }
        labels.extend(tools(&manifest, &section).into_iter().map(|tool| tool.binary));
    }

    assert!(!labels.is_empty(), "no label to measure");
    for label in labels {
        assert!(
            label.chars().count() <= Doctor::LABEL_WIDTH,
            "{label:?} is {} wide, past the {} column",
            label.chars().count(),
            Doctor::LABEL_WIDTH
        );
    }
}

/// Setup cannot install these, so the row a developer reads has to name the version to install.
///
/// Asserted through the rendered row rather than the hint string: the hint is built from the
/// entry, so comparing it against that entry would be true by construction.
#[test]
fn a_system_tool_row_names_the_version_to_install() {
    let root = repo_root();
    let manifest = load_manifest(&root);
    let mut checked = 0_usize;
    for tool in tools(&manifest, "system_tools") {
        let mut checks = Doctor::new(Palette::new(false));
        repo_policy::dev_env::check_system_tool_at(&root, &mut checks, &tool, None);
        let row = checks.rows().last().expect("the check printed a row");
        assert!(row.contains(&tool.version), "{}: the row omits the version to install: {row}", tool.binary);
        checked += 1;
    }
    assert!(checked > 0, "the manifest pins no system tool to check");
}
