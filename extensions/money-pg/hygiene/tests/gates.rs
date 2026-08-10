mod support;

use serde_json::json;

#[test]
fn gates_compose_every_required_check() {
    let lane = support::lane_root();
    let lane_dump = support::just_dump(&lane);
    let offline = support::recipe_dependencies(&lane_dump, "gate-offline");
    for required in ["doc-pg", "deny", "test-hygiene", "miri-payload", "selftest-all"] {
        assert!(offline.contains(&required), "gate-offline must depend on {required}");
    }

    // Every negative control the lane owns, gathered in one recipe, and that recipe reached by a
    // required check. A control behind a local aggregate only is not CI coverage: it can stop
    // firing and merge green, which is indistinguishable from a control that cannot fail.
    let selftests = support::recipe_dependencies(&lane_dump, "selftest-all");
    for required in [
        "test-regress-selftest",
        "artifact-selftest",
        "exactly-one-selftest",
        "workspace-lock-selftest",
        "numa-selftest",
    ] {
        assert!(selftests.contains(&required), "selftest-all must depend on {required}");
    }
    let workflow = support::read(support::repository_root().join(".github/workflows/on-pr-synced.yml"));
    assert!(
        workflow.contains("just pg selftest-all"),
        "a CI job must run `just pg selftest-all`; controls only `gate-offline` reaches are not \
         covered by any required check"
    );
    assert!(
        support::recipe_dependencies(&lane_dump, "gate-pg").contains(&"gate-offline"),
        "gate-pg must compose gate-offline"
    );

    let repository_dump = support::just_dump(&support::repository_root());
    assert!(
        support::recipe_dependencies(&repository_dump, "lint-all").contains(&"scrub"),
        "root lint-all must include the repository scrub"
    );
    assert!(
        support::recipe_body(&repository_dump, "gate").contains("just lint-all"),
        "root gate must run lint-all"
    );
}

#[test]
fn release_gate_covers_one_immutable_deployable_artifact() {
    let dump = support::just_dump(&support::lane_root());
    let release = support::recipe_body(&dump, "gate-pg-release");
    for required in [
        "export KMONEY_USE_LOCAL_CORE=0",
        "just gate-pg",
        "just _yb-ab-ref",
        "node-image.sh",
        "YB_REQUIRE_BAKED=1",
        "run-yb-regress.sh",
        "run-yb-cluster.sh",
        "run-yb-concurrent.sh",
        "run-yb-readreplica.sh",
        "run-yb-restore.sh",
        "rs_noop",
    ] {
        assert!(release.contains(required), "gate-pg-release must execute {required}");
    }
    assert!(
        !release.contains("just yb-ab"),
        "release gate must pass its resolved image to _yb-ab-ref, not resolve a mutable tag again"
    );
}

fn routes_scratch_through_run_root(body: &str) -> bool {
    const DEFAULT: &str = r#"RUN_ROOT="${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}""#;
    body.contains(DEFAULT)
        && body
            .lines()
            .filter(|line| !line.contains(DEFAULT))
            .all(|line| !line.contains("kamu-money-pg/yb/out"))
}

#[test]
fn artifact_recipes_use_the_configured_run_root() {
    let dump = support::just_dump(&support::lane_root());
    for recipe in ["yb-build", "yb-native", "_yb-ab-ref", "gate-pg-release"] {
        let body = support::recipe_body(&dump, recipe);
        assert!(
            routes_scratch_through_run_root(&body),
            "{recipe} must derive scratch paths from KMONEY_RUN_ROOT:\n{body}"
        );
    }

    let bad = r#"
        RUN_ROOT="${KMONEY_RUN_ROOT:-kamu-money-pg/yb/out}"
        docker build -o kamu-money-pg/yb/out .
    "#;
    assert!(!routes_scratch_through_run_root(bad), "positive control must reject a hard-coded path");
}

#[test]
fn structured_recipe_body_excludes_comments() {
    let dump = json!({
        "recipes": {
            "gate": {
                "body": [
                    ["#!/usr/bin/env bash"],
                    ["# run-yb-restore.sh explains the requirement"],
                    ["run-yb-regress.sh"]
                ]
            }
        }
    });
    let body = support::recipe_body(&dump, "gate");
    assert!(body.contains("run-yb-regress.sh"));
    assert!(
        !body.contains("run-yb-restore.sh"),
        "comment text must not satisfy an executable-composition assertion"
    );
}

fn captures_status_by_disabling_set_e(logical_line: &str) -> bool {
    logical_line.find("| tee").is_some_and(|pipe| logical_line[pipe..].contains("||"))
}

#[test]
fn gates_do_not_disable_set_e_while_capturing_output() {
    for bad in [r#"} 2>&1 | tee "$LOG" || rc=$?"#, r#"} 2>&1 | tee "$LOG" || true"#] {
        assert!(captures_status_by_disabling_set_e(bad));
    }
    for good in [r#"} 2>&1 | tee "$LOG""#, r#"rc=${PIPESTATUS[0]}"#, r#"command_that_may_fail || rc=$?"#] {
        assert!(!captures_status_by_disabling_set_e(good));
    }

    let root = support::lane_root();
    let mut files = support::tracked_files(Some("*.sh"));
    files.push("Justfile".into());
    let mut offenders = Vec::new();
    for relative in files {
        for (line, logical) in support::logical_lines(&support::read(root.join(&relative))) {
            if !logical.trim_start().starts_with('#') && captures_status_by_disabling_set_e(&logical) {
                offenders.push(format!("{}:{line}: {}", relative.display(), logical.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a gate body in an `||` list runs with set -e disabled; capture PIPESTATUS after the pipeline:\n{}",
        offenders.join("\n")
    );
}
