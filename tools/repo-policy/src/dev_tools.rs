//! The pinned-version manifest, `.config/dev-tools.json`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// Every version this repository pins.
#[derive(Debug, Deserialize)]
pub struct DevTools {
    pub rust: Rust,
    pub cargo_tools: Vec<Tool>,
    pub node_tools: Vec<NodeTool>,
    pub system_tools: Vec<Tool>,
    pub ci_only_tools: BTreeMap<String, String>,
}

/// Three different Rust versions. `primary` is the toolchain CI installs for the public
/// workspace and which pins the compile-fail goldens. `msrv` is the floor the published
/// manifests declare and CI tests exactly. `lane` is the excluded extension lane's channel,
/// which pgrx requires and which differs from `primary` whenever the public workspace moves
/// first.
///
/// Each is a view of a file that some tool honours and this manifest cannot: `rust-toolchain.toml`
/// for the two channels, `Cargo.toml` for the floor. `tests/pins.rs` holds them equal.
#[derive(Debug, Deserialize)]
pub struct Rust {
    pub primary: String,
    pub msrv: String,
    pub lane: String,
    pub primary_components: Vec<String>,
    pub msrv_components: Vec<String>,
    pub primary_targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Tool {
    /// The name `taiki-e/install-action` knows the tool by, which is not always the binary name.
    pub workflow_name: String,
    pub binary: String,
    pub version: String,
    #[serde(default)]
    pub install_hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NodeTool {
    pub package: String,
    pub binary: String,
    pub version: String,
}

/// Failure to read or decode the manifest.
#[derive(Debug)]
pub enum Error {
    Read { path: String, source: std::io::Error },
    Decode { path: String, source: serde_json::Error },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "cannot read {path}: {source}"),
            Self::Decode { path, source } => write!(f, "cannot decode {path}: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Decode { source, .. } => Some(source),
        }
    }
}

impl DevTools {
    /// The manifest's path, relative to the repository root.
    pub const PATH: &'static str = ".config/dev-tools.json";

    /// Read the manifest from a repository root.
    pub fn load(repo_root: &Path) -> Result<Self, Error> {
        let path = repo_root.join(Self::PATH);
        let shown = path.display().to_string();
        let text =
            std::fs::read_to_string(&path).map_err(|source| Error::Read { path: shown.clone(), source })?;
        serde_json::from_str(&text).map_err(|source| Error::Decode { path: shown, source })
    }

    /// Every tool a workflow can install, as `name@version` keyed by its output name.
    ///
    /// `taiki-e/install-action` takes this spelling directly, so a job names the tools it wants
    /// and takes their versions from here.
    pub fn install_specs(&self) -> BTreeMap<String, String> {
        self.cargo_tools
            .iter()
            .map(|tool| {
                (
                    output_name(&format!("tool_{}", tool.workflow_name)),
                    format!("{}@{}", tool.workflow_name, tool.version),
                )
            })
            .collect()
    }
}

/// GitHub Actions parses a hyphen in an output or environment name as subtraction, so an output
/// name carries underscores only.
pub fn output_name(raw: &str) -> String {
    raw.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("the crate sits two levels below the repository root")
    }

    #[test]
    fn the_manifest_decodes() {
        let tools = DevTools::load(repo_root()).expect("dev-tools.json decodes");
        assert!(!tools.rust.primary.is_empty());
        assert!(!tools.rust.msrv.is_empty());
        assert!(!tools.cargo_tools.is_empty());
    }

    #[test]
    fn output_names_never_carry_a_hyphen() {
        let tools = DevTools::load(repo_root()).expect("dev-tools.json decodes");
        let specs = tools.install_specs();
        assert!(!specs.is_empty(), "no cargo tool produced an install spec");
        for name in specs.keys() {
            assert!(
                !name.contains('-'),
                "output name {name} carries a hyphen, which Actions parses as subtraction",
            );
        }
    }

    #[test]
    fn an_install_spec_pairs_the_workflow_name_with_its_version() {
        let tools = DevTools::load(repo_root()).expect("dev-tools.json decodes");
        let specs = tools.install_specs();
        for tool in &tools.cargo_tools {
            let key = output_name(&format!("tool_{}", tool.workflow_name));
            let spec = specs.get(&key).unwrap_or_else(|| panic!("{key} is emitted"));
            assert_eq!(*spec, format!("{}@{}", tool.workflow_name, tool.version));
        }
    }
}
