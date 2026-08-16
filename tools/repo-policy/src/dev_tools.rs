//! The pinned-version manifest, `.config/dev-tools.json`.

use std::path::Path;

use serde::Deserialize;

/// The versions this repository pins. Only the Rust block is modelled; a field gains a type
/// here when a check reads it.
#[derive(Debug, Deserialize)]
pub struct DevTools {
    pub rust: Rust,
}

/// Three Rust versions, each a different fact. `primary` is the toolchain CI installs for the
/// public workspace and which pins the compile-fail goldens. `msrv` is the floor the published
/// manifests declare and CI tests exactly. `lane` is the excluded extension lane's channel,
/// which pgrx requires.
///
/// Each is a view of a file some tool honours and this manifest cannot: `rust-toolchain.toml`
/// for the channels, `Cargo.toml` for the floor. `tests/pins.rs` holds them equal.
#[derive(Debug, Deserialize)]
pub struct Rust {
    pub primary: String,
    pub msrv: String,
    pub lane: String,
    pub primary_components: Vec<String>,
    pub msrv_components: Vec<String>,
    pub lane_components: Vec<String>,
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

    /// Read the manifest from a repository root.
    pub fn load(repo_root: &Path) -> Result<Self, Error> {
        let path = repo_root.join(Self::PATH);
        let shown = path.display().to_string();
        let text =
            std::fs::read_to_string(&path).map_err(|source| Error::Read { path: shown.clone(), source })?;
        serde_json::from_str(&text).map_err(|source| Error::Decode { path: shown, source })
    }
}
