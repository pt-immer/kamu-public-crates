//! Run the public-workspace gate.

use std::process::ExitCode;

use repo_policy::dev_env::load_manifest;
use repo_policy::gate::{run, stages};
use repo_policy::repo_root;

fn main() -> ExitCode {
    let root = repo_root();
    let manifest = load_manifest(&root);
    let status = run(&root, &stages(&manifest.rust.msrv));
    ExitCode::from(u8::try_from(status).unwrap_or(1))
}
