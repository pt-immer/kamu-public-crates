//! crates.io sparse-index probes.
//!
//! Three outcomes, not two: a version that is published, one that is not, and an index that could
//! not be read. `on-release-published.yml` branches on the third, so it is a distinct exit code.

use std::process::Command;
use std::time::{Duration, Instant};

use semver::{Version, VersionReq};

const INDEX_ROOT: &str = "https://index.crates.io";
const USER_AGENT: &str = "kamu-public-crates-ci (https://github.com/pt-immer/kamu-public-crates)";
const ATTEMPTS: u32 = 3;
const POLL: Duration = Duration::from_secs(15);

pub const EXIT_ANSWERED_NO: u8 = 1;
pub const EXIT_UNREADABLE: u8 = 2;

/// The index could not be read. Never a statement about what the registry holds.
#[derive(Debug)]
pub struct Unreadable(pub String);

impl std::fmt::Display for Unreadable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Entry {
    Found(Vec<Release>),
    /// The registry served a 404: this crate has never been published.
    Absent,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub yanked: bool,
}

pub fn index_path(name: &str) -> String {
    let name = name.to_lowercase();
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

/// Decode the newline-delimited JSON the sparse index serves.
pub fn parse(body: &str) -> Result<Vec<Release>, Unreadable> {
    body.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line)
                .map_err(|error| Unreadable(format!("index line is not JSON: {error}")))?;
            let raw = row["vers"].as_str().ok_or_else(|| Unreadable("index line has no 'vers'".into()))?;
            Ok(Release {
                version: Version::parse(raw)
                    .map_err(|error| Unreadable(format!("index version {raw:?}: {error}")))?,
                yanked: row["yanked"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// Fetch one index entry, retrying only what is transient.
///
/// Shells to `curl` for the reason [`crate::tracked`] shells to `git`: the tool is already
/// required, and a linked TLS stack would land in the release-critical path.
pub fn fetch(name: &str) -> Result<Entry, Unreadable> {
    let url = format!("{INDEX_ROOT}/{}", index_path(name));
    let mut last = String::new();

    for attempt in 1..=ATTEMPTS {
        let output = Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--max-time",
                "20",
                "--user-agent",
                USER_AGENT,
                "--write-out",
                "\n%{http_code}",
                &url,
            ])
            .output()
            .map_err(|error| Unreadable(format!("cannot run curl: {error}")))?;

        if output.status.success() {
            let combined = String::from_utf8_lossy(&output.stdout);
            let (body, status) =
                combined.rsplit_once('\n').ok_or_else(|| Unreadable("curl wrote no status code".into()))?;
            match status.trim() {
                "200" => return parse(body).map(Entry::Found),
                "404" => return Ok(Entry::Absent),
                code @ ("429" | "500" | "502" | "503" | "504") => {
                    last = format!("crates.io returned HTTP {code}");
                }
                code => {
                    return Err(Unreadable(format!("crates.io returned HTTP {code} for {name}")));
                }
            }
        } else {
            last =
                format!("curl exited {}: {}", output.status, String::from_utf8_lossy(&output.stderr).trim());
        }

        if attempt < ATTEMPTS {
            std::thread::sleep(Duration::from_secs(u64::from(attempt)));
        }
    }

    Err(Unreadable(format!("crates.io lookup failed after {ATTEMPTS} attempts for {name}: {last}")))
}

/// Whether a version satisfies a Cargo requirement.
pub fn matches(requirement: &str, version: &str) -> Result<bool, Unreadable> {
    let request = requirement_of(requirement)?;
    let version = Version::parse(version)
        .map_err(|error| Unreadable(format!("invalid version {version:?}: {error}")))?;
    Ok(request.matches(&version))
}

fn requirement_of(requirement: &str) -> Result<VersionReq, Unreadable> {
    VersionReq::parse(requirement)
        .map_err(|error| Unreadable(format!("invalid requirement {requirement:?}: {error}")))
}

/// The highest non-yanked published version satisfying a Cargo requirement.
pub fn latest_satisfying(name: &str, requirement: &str) -> Result<Option<Version>, Unreadable> {
    let request = requirement_of(requirement)?;
    let releases = match fetch(name)? {
        Entry::Absent => return Ok(None),
        Entry::Found(releases) => releases,
    };
    Ok(pick(releases, &request))
}

/// The highest release a requirement admits. A yanked release satisfies nothing.
fn pick(releases: Vec<Release>, request: &VersionReq) -> Option<Version> {
    releases
        .into_iter()
        .filter(|release| !release.yanked && request.matches(&release.version))
        .map(|release| release.version)
        .max()
}

/// Poll until a satisfying version is published, or the deadline passes.
///
/// The sparse index lags a publish, so an absent version is not yet a final answer. A deadline
/// spent against an index that never answered reports unreadable rather than absent.
pub fn require(name: &str, requirement: &str, wait: Duration) -> Result<bool, Unreadable> {
    let deadline = Instant::now() + wait;
    let mut unreadable: Option<Unreadable>;

    loop {
        match latest_satisfying(name, requirement) {
            Ok(Some(_)) => return Ok(true),
            Ok(None) => unreadable = None,
            Err(error) => unreadable = Some(error),
        }

        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline - now;
        eprintln!(
            "crates.io: {name} does not yet satisfy {requirement:?}; {}s of wait left",
            remaining.as_secs()
        );
        std::thread::sleep(POLL.min(remaining));
    }

    match unreadable {
        Some(error) => Err(error),
        None => Ok(false),
    }
}

/// Whether an exact version is absent from the registry.
pub fn is_absent(name: &str, version: &Version) -> Result<bool, Unreadable> {
    match fetch(name)? {
        Entry::Absent => Ok(true),
        Entry::Found(releases) => Ok(!releases.iter().any(|release| &release.version == version)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_paths_bucket_by_name_length() {
        assert_eq!("1/a", index_path("a"));
        assert_eq!("2/ab", index_path("ab"));
        assert_eq!("3/a/abc", index_path("abc"));
        assert_eq!("ka/mu/kamu-money-core", index_path("kamu-money-core"));
    }

    #[test]
    fn cargo_requirements_keep_their_cargo_meaning() {
        for (requirement, accepted, rejected) in [
            ("0.1", vec!["0.1.0", "0.1.99"], vec!["0.0.99", "0.2.0", "1.0.0"]),
            ("2", vec!["2.0.0", "2.99.0"], vec!["1.99.0", "3.0.0"]),
            (">=1.2, <2", vec!["1.2.0", "1.9.9"], vec!["1.1.9", "2.0.0"]),
            ("~1.2", vec!["1.2.0", "1.2.9"], vec!["1.3.0"]),
            ("1.2.*", vec!["1.2.0", "1.2.9"], vec!["1.3.0"]),
            ("=1.2.3", vec!["1.2.3"], vec!["1.2.4"]),
        ] {
            for version in accepted {
                assert!(
                    matches(requirement, version).expect("requirement parses"),
                    "{requirement} should accept {version}"
                );
            }
            for version in rejected {
                assert!(
                    !matches(requirement, version).expect("requirement parses"),
                    "{requirement} should reject {version}"
                );
            }
        }
    }

    #[test]
    fn a_prerelease_needs_a_requirement_that_names_one() {
        assert!(!matches("1.2.3", "1.2.3-rc.1").expect("requirement parses"));
        assert!(matches(">=1.2.3-rc.1, <1.2.3", "1.2.3-rc.2").expect("requirement parses"));
    }

    #[test]
    fn yanked_releases_are_decoded_but_never_satisfy() {
        let releases = parse(
            "{\"name\":\"x\",\"vers\":\"1.0.0\",\"yanked\":false}\n\
             {\"name\":\"x\",\"vers\":\"1.1.0\",\"yanked\":true}\n",
        )
        .expect("index decodes");
        assert_eq!(
            vec![Version::parse("1.0.0").unwrap(), Version::parse("1.1.0").unwrap()],
            releases.iter().map(|r| r.version.clone()).collect::<Vec<_>>()
        );
        assert!(releases[1].yanked);
    }

    fn releases(rows: &[(&str, bool)]) -> Vec<Release> {
        rows.iter()
            .map(|(raw, yanked)| Release {
                version: Version::parse(raw).expect("test version parses"),
                yanked: *yanked,
            })
            .collect()
    }

    #[test]
    fn a_yanked_release_satisfies_nothing() {
        let request = VersionReq::parse("1").expect("requirement parses");
        assert_eq!(
            Some(Version::parse("1.0.0").unwrap()),
            pick(releases(&[("1.0.0", false), ("1.1.0", true)]), &request)
        );
        assert_eq!(None, pick(releases(&[("1.1.0", true)]), &request));
    }

    #[test]
    fn the_highest_admitted_release_wins_regardless_of_index_order() {
        let request = VersionReq::parse("0.1").expect("requirement parses");
        assert_eq!(
            Some(Version::parse("0.1.9").unwrap()),
            pick(releases(&[("0.1.9", false), ("0.1.2", false), ("0.2.0", false)]), &request)
        );
    }

    #[test]
    fn an_undecodable_index_line_is_unreadable_not_empty() {
        assert!(parse("not json at all\n").is_err());
        assert!(parse("{\"name\":\"x\"}\n").is_err());
    }
}
