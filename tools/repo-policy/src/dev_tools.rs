//! The pinned-version manifest, `.config/dev-tools.json`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// The suffix every tool section's key carries.
///
/// Sections are found by this shape rather than listed, so a section added to the manifest is
/// one a reader has to account for rather than one three separate lists silently omit.
pub const TOOL_SECTION_SUFFIX: &str = "_tools";

/// The versions this repository pins. A field gains a type here when a check reads it.
#[derive(Debug)]
pub struct DevTools {
    pub rust: Rust,
    /// Each tool section, under its own name, ordered by that name.
    pub tools: BTreeMap<String, BTreeMap<String, Tool>>,
}

impl<'de> Deserialize<'de> for DevTools {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Manifest;

        impl<'de> serde::de::Visitor<'de> for Manifest {
            type Value = DevTools;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("the pinned-version manifest")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<DevTools, M::Error> {
                let mut rust = None;
                let mut tools = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if key == "rust" {
                        rust = Some(map.next_value()?);
                    } else if key.ends_with(TOOL_SECTION_SUFFIX) {
                        tools.insert(key, map.next_value()?);
                    } else {
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                let rust = rust.ok_or_else(|| serde::de::Error::missing_field("rust"))?;
                if tools.is_empty() {
                    return Err(serde::de::Error::custom("the manifest names no tool section"));
                }
                Ok(DevTools { rust, tools })
            }
        }

        deserializer.deserialize_map(Manifest)
    }
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

    /// The sections that define a tool. Two of them would be two pins for one name, and the
    /// one a reader checked would be whichever they looked at first.
    pub fn tool(&self, name: &str) -> Vec<(&str, &Tool)> {
        self.tools
            .iter()
            .filter_map(|(section, tools)| tools.get(name).map(|tool| (section.as_str(), tool)))
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
