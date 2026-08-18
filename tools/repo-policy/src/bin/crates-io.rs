//! Registry probes for the release workflow.
//!
//! Exit codes are the interface: 0 answered yes, 1 answered no, 2 could not read the index.

use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use repo_policy::registry::{self, EXIT_ANSWERED_NO, EXIT_UNREADABLE};
use semver::Version;

#[derive(Parser)]
#[command(about = "crates.io sparse-index probes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fail unless a non-yanked published version satisfies the requirement.
    Require {
        crate_name: String,
        requirement: String,
        /// Poll until this deadline before reporting the version absent.
        #[arg(long, default_value_t = 0)]
        wait_seconds: u64,
    },
    /// Fail if the exact version is already published.
    EnsureAbsent { crate_name: String, version: String },
    /// Report whether each version satisfies the requirement.
    Matches {
        requirement: String,
        #[arg(required = true)]
        versions: Vec<String>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("crates-io: {error}");
            ExitCode::from(EXIT_UNREADABLE)
        }
    }
}

fn run(command: Command) -> Result<ExitCode, registry::Unreadable> {
    match command {
        Command::Require { crate_name, requirement, wait_seconds } => {
            if registry::require(&crate_name, &requirement, Duration::from_secs(wait_seconds))? {
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("crates.io: {crate_name} has no version satisfying {requirement:?}");
                Ok(ExitCode::from(EXIT_ANSWERED_NO))
            }
        }
        Command::EnsureAbsent { crate_name, version } => {
            let target = Version::parse(&version)
                .map_err(|error| registry::Unreadable(format!("invalid version {version:?}: {error}")))?;
            if registry::is_absent(&crate_name, &target)? {
                println!("crates.io: {crate_name} {target} is absent");
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("crates.io: {crate_name} {target} is already published");
                Ok(ExitCode::from(EXIT_ANSWERED_NO))
            }
        }
        Command::Matches { requirement, versions } => {
            let mut all = true;
            for version in &versions {
                let satisfied = registry::matches(&requirement, version)?;
                println!("{version}={satisfied}");
                all &= satisfied;
            }
            Ok(if all { ExitCode::SUCCESS } else { ExitCode::from(EXIT_ANSWERED_NO) })
        }
    }
}
