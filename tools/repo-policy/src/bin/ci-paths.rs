//! Classify a diff into CI classes, or prove every tracked path has an owner.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use repo_policy::ci_paths::{Unclassified, classify_paths};

#[derive(Parser)]
#[command(about = "classify changed paths for CI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Classify a diff and write GitHub Actions outputs.
    Emit {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, env = "GITHUB_OUTPUT")]
        github_output: Option<String>,
    },
    /// Prove every currently tracked path has at least one owner.
    CheckTracked,
    /// Classify explicit paths.
    Classify {
        #[arg(required = true)]
        paths: Vec<String>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ci-paths: {message}");
            ExitCode::FAILURE
        }
    }
}

/// NUL-delimited paths from a git command, so a path containing a newline stays one path.
fn git_paths(arguments: &[&str]) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(repo_policy::repo_root())
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

fn run(command: Command) -> Result<(), String> {
    let (paths, destination) = match command {
        Command::Emit { base, head, github_output } => {
            let destination =
                github_output.ok_or("--github-output or GITHUB_OUTPUT is required".to_string())?;
            let paths = git_paths(&["diff", "--name-only", "-z", "--find-renames", &base, &head, "--"])?;
            (paths, Some(destination))
        }
        Command::CheckTracked => (git_paths(&["ls-files", "-z"])?, None),
        Command::Classify { paths } => {
            let classes = classify_paths(&paths).map_err(render)?;
            for (name, fired) in classes {
                println!("{name}={fired}");
            }
            return Ok(());
        }
    };

    let classes = classify_paths(&paths).map_err(render)?;
    if let Some(destination) = destination {
        let rendered: String = classes.iter().map(|(name, fired)| format!("{name}={fired}\n")).collect();
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&destination)
            .and_then(|mut file| std::io::Write::write_all(&mut file, rendered.as_bytes()))
            .map_err(|error| format!("cannot append to {destination}: {error}"))?;
    }

    println!("ci-paths: classified {} path(s)", paths.len());
    Ok(())
}

fn render(error: Unclassified) -> String {
    error.to_string()
}
