//! The public-workspace gate: which stages run, how they overlap, and what the run reports.

use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Instant;

/// Stages within a group run serially because they share one Cargo build directory and would
/// otherwise queue on its lock. The groups run concurrently.
///
/// Only a group that already owns a disjoint build directory is split out: `cov` drives
/// cargo-llvm-cov into `target/llvm-cov-target`, and `misc` either builds a separate workspace or
/// does not build at all. The MSRV and cross-target stages stay in `host` deliberately: escaping
/// the same lock would need new target directories, gigabytes of duplicated artifacts cold on
/// first use, to overlap 21 seconds of a 430-second run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Host,
    Cov,
    Misc,
}

pub struct Stage {
    pub name: String,
    pub command: String,
    pub group: Group,
}

/// Every stage the gate runs, in the order it reports them.
///
/// The MSRV stage reads its channel from the manifest rather than stating one: a toolchain
/// literal here would be a second home for a version `.config/dev-tools.json` already owns.
pub fn stages(msrv: &str) -> Vec<Stage> {
    let stage = |name: &str, command: String, group: Group| Stage { name: name.to_owned(), command, group };
    let just = |name: &str, group: Group| stage(name, format!("just {name}"), group);

    vec![
        just("lint-all", Group::Host),
        just("test-all", Group::Host),
        just("test-money", Group::Host),
        just("test-policy", Group::Host),
        stage(
            &format!("msrv({msrv})"),
            format!(
                "cargo +{msrv} nextest run --workspace -E 'not binary(compile_fail)' && \
                 cargo +{msrv} test --workspace --doc --quiet"
            ),
            Group::Host,
        ),
        just("cov-all", Group::Cov),
        just("doc", Group::Host),
        just("build-nostd", Group::Host),
        just("build-wasm", Group::Host),
        just("build-wasm-snap", Group::Host),
        just("check-worker-example", Group::Misc),
        just("check-examples", Group::Host),
        just("deny", Group::Misc),
    ]
}

pub struct Outcome {
    pub index: usize,
    pub code: i32,
    pub seconds: u64,
    pub output: String,
}

/// Run every stage, overlapping the groups, and report as each lands.
///
/// Completion order, so a five-minute barrier still shows progress. The ordered view is the
/// failure replay below and `VERBOSE=1`.
pub fn run(root: &Path, stages: &[Stage]) -> i32 {
    let started = Instant::now();
    let (sender, receiver) = mpsc::channel::<Outcome>();

    std::thread::scope(|scope| {
        for group in [Group::Host, Group::Cov, Group::Misc] {
            let sender = sender.clone();
            scope.spawn(move || {
                for (index, stage) in stages.iter().enumerate() {
                    if stage.group != group {
                        continue;
                    }
                    let began = Instant::now();
                    let output = Command::new("bash").args(["-c", &stage.command]).current_dir(root).output();
                    let (code, text) = match output {
                        Ok(output) => (
                            output.status.code().unwrap_or(1),
                            format!(
                                "{}{}",
                                String::from_utf8_lossy(&output.stdout),
                                String::from_utf8_lossy(&output.stderr)
                            ),
                        ),
                        Err(error) => (1, format!("cannot run {}: {error}", stage.command)),
                    };
                    let seconds = began.elapsed().as_secs();
                    println!("  {}  {seconds:>5}s  {}", if code == 0 { "PASS" } else { "FAIL" }, stage.name);
                    let _ = sender.send(Outcome { index, code, seconds, output: text });
                }
            });
        }
        drop(sender);
    });

    let mut outcomes: Vec<Option<Outcome>> = (0..stages.len()).map(|_| None).collect();
    for outcome in receiver {
        let index = outcome.index;
        outcomes[index] = Some(outcome);
    }
    println!("  ----  {:>5}s  total", started.elapsed().as_secs());

    let verbose = std::env::var("VERBOSE").is_ok_and(|value| value == "1");
    // A stage with no outcome means its group died before recording one. That is a failure, not
    // an absence.
    let failed = outcomes.iter().any(|outcome| outcome.as_ref().is_none_or(|o| o.code != 0));

    for (index, stage) in stages.iter().enumerate() {
        let Some(outcome) = &outcomes[index] else {
            println!("\n=== {} (NO VERDICT) ===", stage.name);
            continue;
        };
        if verbose {
            println!("\n=== {} ===\n{}", stage.name, outcome.output);
        } else if failed && outcome.code != 0 {
            println!("\n=== {} (FAILED) ===\n{}", stage.name, outcome.output);
        }
    }

    if lane_has_changes(root) {
        println!("\n  NOTE  extensions/money-pg has changes this gate did NOT cover.");
        println!("        Run 'just gate-all' before pushing them.");
    }

    i32::from(failed)
}

/// Whether the excluded lane carries changes this Docker-free gate did not cover.
fn lane_has_changes(root: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all", "--", "extensions/money-pg"])
        .current_dir(root)
        .output()
        .is_ok_and(|output| !output.stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_msrv_stage_names_the_channel_it_was_given() {
        let stages = stages("1.94.0");
        let msrv = stages.iter().find(|stage| stage.name.starts_with("msrv(")).expect("an MSRV stage");
        assert_eq!("msrv(1.94.0)", msrv.name);
        assert!(msrv.command.contains("cargo +1.94.0 nextest run"));
        assert!(msrv.command.contains("cargo +1.94.0 test --workspace --doc"));
    }

    #[test]
    fn no_stage_states_a_toolchain_of_its_own() {
        for stage in stages("9.9.9") {
            if stage.name.starts_with("msrv(") {
                continue;
            }
            assert!(!stage.command.contains("cargo +"), "{} pins a toolchain", stage.name);
        }
    }

    #[test]
    fn every_group_carries_at_least_one_stage() {
        let stages = stages("1.0.0");
        for group in [Group::Host, Group::Cov, Group::Misc] {
            assert!(
                stages.iter().any(|stage| stage.group == group),
                "{group:?} schedules nothing, so its thread does nothing"
            );
        }
    }
}
