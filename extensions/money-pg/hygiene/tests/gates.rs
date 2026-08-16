mod support;

use serde_json::json;

#[test]
fn gates_compose_every_required_check() {
    let lane = support::lane_root();
    let lane_dump = support::just_dump(&lane);
    // EQUALITY, not containment. A subset leaves the unlisted dependencies free to be dropped
    // while this test goes on passing, which is how a developer gate becomes quietly weaker than
    // the required check it stands in for. Adding a check here is meant to fail until it is
    // named.
    let offline = support::recipe_dependencies(&lane_dump, "gate-offline");
    assert_eq!(
        offline,
        [
            "fmt-check",
            "lint",
            "deny",
            "doc-pg",
            "doc-gate-selftest",
            "test-hygiene",
            "miri-payload",
            "selftest-all",
        ],
        "gate-offline no longer composes exactly the checks the lane gate promises"
    );

    // Every negative control the lane owns, gathered in one recipe, and that recipe reached by a
    // required check. A control behind a local aggregate only is not CI coverage: it can stop
    // firing and merge green, which is indistinguishable from a control that cannot fail.
    let selftests = support::recipe_dependencies(&lane_dump, "selftest-all");
    for required in [
        "test-regress-selftest",
        "artifact-selftest",
        "exactly-one-selftest",
        "require-cache-exporter-selftest",
        "workspace-lock-selftest",
        "numa-selftest",
    ] {
        assert!(selftests.contains(&required), "selftest-all must depend on {required}");
    }
    // WHICH recipes CI must reach is the lane's own claim, so it is asserted here. HOW a step is
    // spelled is workflow shape, and `scripts/test_workflows.py` owns that -- a `- run:` here
    // and a `- name:` with its own `run:` are the same step, and this file has no business
    // preferring one.
    let workflow = support::read(support::repository_root().join(".github/workflows/on-pr-synced.yml"));
    for required in ["just pg selftest-all", "just pg doc-pg", "just pg doc-gate-selftest"] {
        assert!(
            workflow.contains(required),
            "a CI job must run `{required}`; no CI job runs `gate-offline`, so composing it there \
             is not what reaches a required check"
        );
    }
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
        "rs_noop",
    ] {
        assert!(release.contains(required), "gate-pg-release must execute {required}");
    }

    // AND MUST NOT RUN THE DEPLOYMENT SUITES. Replication factor, tablet placement, read replicas
    // and dump/restore are YugabyteDB's behaviour: a kmoney payload is opaque bytes to DocDB, so
    // asserting that raft copies it, a split relocates it and a replica serves it tests Yugabyte,
    // and charges this gate hours to do it. They keep their recipes under `test-yb-deployment`.
    for operational in
        ["run-yb-cluster.sh", "run-yb-concurrent.sh", "run-yb-readreplica.sh", "run-yb-restore.sh"]
    {
        assert!(
            !release.contains(operational),
            "gate-pg-release runs {operational}, which proves YugabyteDB's behaviour rather than \
             the extension's; it belongs to `just pg test-yb-deployment`"
        );
    }

    // Dropped from the gate is not dropped from the repository: each stays runnable, and one
    // recipe still runs them together.
    let deployment = support::recipe_dependencies(&dump, "test-yb-deployment");
    for required in ["test-yb-cluster", "test-yb-readreplica", "test-yb-concurrent", "test-yb-restore"] {
        assert!(deployment.contains(&required), "test-yb-deployment must compose {required}");
    }
    assert!(
        !release.contains("just yb-ab"),
        "release gate must pass its resolved image to _yb-ab-ref, not resolve a mutable tag again"
    );
}

/// Every feature that gates code must be one the doc gate turns on.
///
/// `--document-private-items` decides what rustdoc KEEPS; the feature list decides what the
/// compiler hands it at all. A `#[cfg(feature = "x")]` block the recipe does not enable is not
/// documented, not link-checked, and not visibly absent.
///
/// `doc-gate-selftest.sh` proves the regions that exist today by planting a link in each. This
/// covers the one it cannot: a region added later. The recipe's `--features` argument is parsed
/// rather than searched, because its body also carries `target/doc/kmoney` and
/// `--no-default-features`, which make a substring test answer yes to `doc`, `test` and `pg`.
#[test]
fn the_doc_gate_compiles_every_feature_gated_block() {
    let mut gated: Vec<String> = Vec::new();
    for path in support::rust_sources_under(&support::lane_root().join("kamu-money-pg/src")) {
        let source = support::read(&path);
        // `cfg_attr` and `cfg!` gate code the same way `cfg` does, so the match is on `cfg`
        // rather than on `#[cfg(`.
        for line in source.lines().filter(|line| line.contains("cfg")) {
            let mut rest = line;
            while let Some(at) = rest.find("feature").map(|at| at + "feature".len()) {
                rest = &rest[at..];
                let Some(quoted) = rest.strip_prefix(" = \"").or_else(|| rest.strip_prefix("=\"")) else {
                    continue;
                };
                let Some((name, tail)) = quoted.split_once('"') else {
                    panic!("{}: unterminated feature literal in `{}`", path.display(), line.trim());
                };
                // `not(feature = "x")` documents the branch taken when x is OFF, so requiring the
                // gate to enable it would demand the opposite of what the code says.
                if !line.contains("not(feature") {
                    gated.push(name.to_owned());
                }
                rest = tail;
            }
        }
    }
    gated.sort_unstable();
    gated.dedup();
    assert!(!gated.is_empty(), "no `cfg(feature = ...)` found -- this guard would pass vacuously");

    let body = support::recipe_body(&support::just_dump(&support::lane_root()), "doc-pg");
    let enabled: Vec<&str> = body
        .split_whitespace()
        .skip_while(|token| *token != "--features")
        .nth(1)
        .unwrap_or_else(|| panic!("doc-pg must pass --features; its body is:\n{body}"))
        .split(',')
        .collect();
    let missing: Vec<&String> = gated.iter().filter(|feature| !enabled.contains(&feature.as_str())).collect();
    assert!(
        missing.is_empty(),
        "doc-pg enables {enabled:?} and so never compiles the code behind {missing:?}. A pgrx \
         major among these cannot simply be added: they are mutually exclusive, so one rustdoc \
         run cannot cover them all and the gap is real."
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
