//! The pinned-version manifest, `.config/dev-tools.json`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// The versions this repository pins. A field gains a type here when a check reads it.
#[derive(Debug, Deserialize)]
pub struct DevTools {
    pub rust: Rust,
    pub cargo_tools: BTreeMap<String, Tool>,
    pub node_tools: BTreeMap<String, Tool>,
    pub system_tools: BTreeMap<String, Tool>,
    pub ci_only_tools: BTreeMap<String, Tool>,
}

/// One pinned tool, keyed by the name it is requested and installed by. An entry names a crate,
/// package, binary or version query only where one differs from that name or the default.
#[derive(Debug, Deserialize)]
pub struct Tool {
    pub version: String,
}

/// Three Rust versions, each a different fact. `primary` is the toolchain CI installs for the
/// public workspace and which pins the compile-fail goldens. `msrv` is the floor the published
/// manifests declare and CI tests exactly. `lane` is the excluded extension lane's channel,
/// which pgrx requires and which CI must select without entering the lane.
///
/// Each is a view of a file some tool honours and this manifest cannot: `rust-toolchain.toml`
/// for the channels, `Cargo.toml` for the floor. `tests/pins.rs` holds them equal.
///
/// Components are stated only for the channels the root `setup` installs. The lane installs
/// its own from its own toolchain file, which is where its prerequisites belong.
#[derive(Debug, Deserialize)]
pub struct Rust {
    pub primary: String,
    pub msrv: String,
    pub lane: String,
    pub primary_components: Vec<String>,
    pub msrv_components: Vec<String>,
    pub primary_targets: Vec<String>,
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

    /// Every tool section, under the name the manifest gives it.
    pub fn tool_sections(&self) -> [(&'static str, &BTreeMap<String, Tool>); 4] {
        [
            ("cargo_tools", &self.cargo_tools),
            ("node_tools", &self.node_tools),
            ("system_tools", &self.system_tools),
            ("ci_only_tools", &self.ci_only_tools),
        ]
    }

    /// The sections that define a tool. Two of them would be two pins for one name, and the
    /// one a reader checked would be whichever they looked at first.
    pub fn tool(&self, name: &str) -> Vec<(&'static str, &Tool)> {
        self.tool_sections()
            .into_iter()
            .filter_map(|(section, tools)| tools.get(name).map(|tool| (section, tool)))
            .collect()
    }

    /// Read the manifest from a repository root.
    pub fn load(repo_root: &Path) -> Result<Self, Error> {
        let path = repo_root.join(Self::PATH);
        let shown = path.display().to_string();
        let text =
            std::fs::read_to_string(&path).map_err(|source| Error::Read { path: shown.clone(), source })?;
        serde_json::from_str(&text).map_err(|source| Error::Decode { path: shown, source })
    }
}
