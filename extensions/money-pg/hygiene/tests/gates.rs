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
        ["fmt-check", "lint", "deny", "doc-pg", "test-hygiene", "miri-payload", "selftest-all"],
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
    // A whole RUN STEP, not a substring anywhere in the file: `just pg doc-pg` is a prefix of
    // every recipe name that extends it, and it matches a commented-out step exactly as well as
    // a live one.
    let workflow = support::read(support::repository_root().join(".github/workflows/on-pr-synced.yml"));
    let steps: Vec<&str> =
        workflow.lines().filter_map(|line| line.trim().strip_prefix("- run: ")).map(str::trim_end).collect();
    for required in ["just pg selftest-all", "just pg doc-pg"] {
        assert!(
            steps.contains(&required),
            "a CI job must run `{required}` as its own step; no CI job runs `gate-offline`, so \
             composing it there is not what reaches a required check"
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

/// Nothing a recipe can leave out may decide whether rustdoc warnings fail.
///
/// Cargo configuration rather than a recipe, because a guard over recipe TEXT is a second copy
/// of the recipe: `cargo +nightly doc`, `@cargo doc`, a backslash-wrapped invocation and a
/// `{{ VARIABLE }}` fragment are one command spelled four ways, and each spelling costs such a
/// guard another clause it can be wrong about. Here the deny is a parsed array with one owner.
#[test]
fn the_lane_denies_every_rustdoc_warning_from_cargo_configuration() {
    let path = support::lane_root().join(".cargo/config.toml");
    let config = support::manifest(&path);
    let flags: Vec<&str> = config
        .get("build")
        .and_then(|build| build.get("rustdocflags"))
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{} must set build.rustdocflags", path.display()))
        .iter()
        .map(|flag| flag.as_str().expect("every rustdocflags entry must be a string"))
        .collect();

    assert!(
        flags.iter().any(|flag| flag.replace("-D warnings", "-Dwarnings") == "-Dwarnings"),
        "{}: build.rustdocflags is {flags:?}, which denies no rustdoc warning -- rustdoc then \
         reports a broken intra-doc link and exits 0",
        path.display()
    );
    // rustdoc takes the LAST verdict on a lint, so an allow appended after the deny is the same
    // vacuity as never denying, reached by adding rather than by removing.
    let allows: Vec<&&str> =
        flags.iter().filter(|flag| flag.starts_with("-A") || flag.starts_with("--allow")).collect();
    assert!(allows.is_empty(), "{}: {allows:?} re-allows what the deny refused", path.display());
}

/// Every feature that gates prose must be one the doc gate turns on.
///
/// `--document-private-items` decides what rustdoc KEEPS; the feature list decides what the
/// compiler hands it at all. A `#[cfg(feature = "x")]` block the recipe does not enable is not
/// documented, not link-checked, and not visibly absent. The features are read out of the source
/// rather than listed here, so a new one fails this test instead of falling silently outside the
/// gate.
#[test]
fn the_doc_gate_compiles_every_feature_gated_block() {
    let mut gated: Vec<String> = Vec::new();
    let mut walk = vec![support::lane_root().join("kamu-money-pg/src")];
    while let Some(directory) = walk.pop() {
        for entry in std::fs::read_dir(&directory).expect("the crate source must be readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                walk.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            for line in support::read(&path).lines().filter(|line| line.contains("#[cfg(")) {
                let mut rest = line;
                while let Some(at) = rest.find("feature = \"") {
                    rest = &rest[at + "feature = \"".len()..];
                    let (name, tail) = rest.split_once('"').expect("a closed feature literal");
                    gated.push(name.to_owned());
                    rest = tail;
                }
            }
        }
    }
    gated.sort_unstable();
    gated.dedup();
    assert!(!gated.is_empty(), "no `#[cfg(feature = ...)]` found -- this guard would pass vacuously");

    let body = support::recipe_body(&support::just_dump(&support::lane_root()), "doc-pg");
    let missing: Vec<&String> = gated.iter().filter(|feature| !body.contains(*feature)).collect();
    assert!(
        missing.is_empty(),
        "doc-pg does not enable {missing:?}, so rustdoc never compiles the prose behind them"
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
