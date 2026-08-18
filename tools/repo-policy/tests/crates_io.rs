//! The exit codes `on-release-published.yml` branches on.
//!
//! Asserted as literals. Comparing against the crate's own constants would compare each value
//! with itself, and the workflow reads the numbers, not the names.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// A `curl` earlier on `PATH` than the real one, answering with a fixed status and body.
fn stub_curl(directory: &Path, http_status: &str, body: &str) -> PathBuf {
    let script = directory.join("curl");
    fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n%s' '{body}' '{http_status}'\n"))
        .expect("stub is writable");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("stub is executable");
    directory.to_path_buf()
}

/// A `curl` that fails to reach anything, the way an outage does.
fn broken_curl(directory: &Path) -> PathBuf {
    let script = directory.join("curl");
    fs::write(&script, "#!/bin/sh\nexit 6\n").expect("stub is writable");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("stub is executable");
    directory.to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("repo-policy-crates-io-{name}"));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("scratch directory is creatable");
    directory
}

fn crates_io(path_prefix: &Path) -> Command {
    let mut command = Command::cargo_bin("crates-io").expect("the binary builds");
    let inherited = std::env::var("PATH").unwrap_or_default();
    command.env("PATH", format!("{}:{inherited}", path_prefix.display()));
    command
}

const PUBLISHED: &str = "{\"name\":\"x\",\"vers\":\"1.0.0\",\"yanked\":false}";

#[test]
fn a_satisfied_requirement_exits_zero() {
    let directory = scratch("satisfied");
    let prefix = stub_curl(&directory, "200", PUBLISHED);
    crates_io(&prefix).args(["require", "x", "=1.0.0"]).assert().code(0);
}

#[test]
fn an_unsatisfied_requirement_exits_one() {
    let directory = scratch("unsatisfied");
    let prefix = stub_curl(&directory, "200", PUBLISHED);
    crates_io(&prefix).args(["require", "x", "=2.0.0"]).assert().code(1);
}

#[test]
fn a_crate_the_registry_never_heard_of_exits_one() {
    let directory = scratch("absent");
    let prefix = stub_curl(&directory, "404", "");
    crates_io(&prefix).args(["require", "x", "=1.0.0"]).assert().code(1);
}

#[test]
fn an_unreadable_index_exits_two_rather_than_reporting_absence() {
    let directory = scratch("unreadable");
    let prefix = broken_curl(&directory);
    crates_io(&prefix)
        .args(["require", "x", "=1.0.0"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("lookup failed"));
}

#[test]
fn waiting_out_an_unreadable_index_still_exits_two() {
    let directory = scratch("unreadable-wait");
    let prefix = broken_curl(&directory);
    crates_io(&prefix).args(["require", "x", "=1.0.0", "--wait-seconds", "1"]).assert().code(2);
}

#[test]
fn an_already_published_version_exits_one() {
    let directory = scratch("present");
    let prefix = stub_curl(&directory, "200", PUBLISHED);
    crates_io(&prefix).args(["ensure-absent", "x", "1.0.0"]).assert().code(1);
}

#[test]
fn an_unpublished_version_exits_zero() {
    let directory = scratch("not-present");
    let prefix = stub_curl(&directory, "200", PUBLISHED);
    crates_io(&prefix).args(["ensure-absent", "x", "2.0.0"]).assert().code(0);
}

#[test]
fn an_unreadable_index_never_reports_a_version_absent() {
    let directory = scratch("absent-unreadable");
    let prefix = broken_curl(&directory);
    crates_io(&prefix).args(["ensure-absent", "x", "1.0.0"]).assert().code(2);
}

#[test]
fn matches_reports_each_version_and_exits_on_the_verdict() {
    let directory = scratch("matches");
    let prefix = stub_curl(&directory, "200", PUBLISHED);
    crates_io(&prefix).args(["matches", "^0.2", "0.2.0"]).assert().code(0).stdout("0.2.0=true\n");
    crates_io(&prefix).args(["matches", "^0.2", "0.3.0"]).assert().code(1).stdout("0.3.0=false\n");
}

#[test]
fn a_5xx_is_retried_and_then_reported_unreadable() {
    let directory = scratch("server-error");
    let prefix = stub_curl(&directory, "503", "");
    crates_io(&prefix).args(["require", "x", "=1.0.0"]).assert().code(2);
}
