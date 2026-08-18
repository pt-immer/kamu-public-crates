//! Install the pinned development environment, or verify it.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use repo_policy::dev_env::{doctor, load_manifest, setup};
use repo_policy::repo_root;

#[derive(Parser)]
#[command(about = "the pinned development environment")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install exact toolchains, targets and repository-local tools, then verify them.
    Setup,
    /// Verify every prerequisite reached by the root gate.
    Doctor,
}

fn main() -> ExitCode {
    let root = repo_root();
    let manifest = load_manifest(&root);
    let status = match Cli::parse().command {
        Command::Setup => setup(&root, &manifest),
        Command::Doctor => doctor(&root, &manifest),
    };
    ExitCode::from(u8::try_from(status).unwrap_or(1))
}
