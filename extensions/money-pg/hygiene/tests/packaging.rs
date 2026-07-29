mod support;

use cargo_metadata::DependencyKind;

#[test]
fn docker_context_excludes_host_state() {
    let root = support::lane_root();
    let ignore = support::read(root.join(".dockerignore"));
    let patterns: Vec<_> =
        ignore.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).collect();

    for required in [".pgrx", "target", "kamu-money-pg/yb/out"] {
        assert!(patterns.contains(&required), ".dockerignore must exclude {required:?}");
    }
}

#[test]
fn docker_builds_share_the_normalized_core_package() {
    let root = support::lane_root();
    let repository = support::repository_root();
    let helper = support::read(root.join("scripts/docker-core-context.sh"));
    for required in ["cargo package", "tests/pg_native_column.rs", "--build-context", "KMONEY_USE_LOCAL_CORE"]
    {
        assert!(helper.contains(required), "Docker context helper must contain {required:?}");
    }

    let core = support::manifest(repository.join("crates/money-core/Cargo.toml"));
    let include = core["package"]["include"].as_array().expect("money-core package.include must be an array");
    assert!(
        include.iter().any(|entry| entry.as_str() == Some("tests/**/*.rs")),
        "the normalized core package must include native-column integration tests"
    );

    for dockerfile in
        ["kamu-money-pg/Dockerfile", "kamu-money-pg/yb/Dockerfile", "kamu-money-pg/yb/Dockerfile.pg15"]
    {
        let source = support::read(root.join(dockerfile));
        for required in ["--from=kamu-money-core", "KMONEY_USE_LOCAL_CORE", "[patch.crates-io]"] {
            assert!(source.contains(required), "{dockerfile} must contain {required:?}");
        }
    }

    for caller in [
        "Justfile",
        "kamu-money-pg/test-matrix.sh",
        "kamu-money-pg/native-driver-test.sh",
        "kamu-money-pg/yb/run-yb-driver.sh",
        "kamu-money-pg/yb/node-image.sh",
        "kamu-money-pg/bench/run-bench-boundary-yb.sh",
    ] {
        for line in support::read(root.join(caller)).lines().filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with('#') && trimmed.contains("docker build")
        }) {
            assert!(
                line.contains("KMONEY_CORE_DOCKER_ARGS"),
                "{caller} has a Docker build without the normalized core context: {line}"
            );
        }
    }

    let dump = support::just_dump(&root);
    let release = support::recipe_body(&dump, "gate-pg-release");
    assert!(
        release.contains("export KMONEY_USE_LOCAL_CORE=0"),
        "release proof must resolve money-core from the registry"
    );

    let workflow = support::read(repository.join(".github/workflows/on-pr-synced.yml"));
    assert!(
        !workflow.contains("money-core-published"),
        "ordinary container CI must not wait for first publication"
    );
}

#[test]
fn every_lane_package_carries_both_license_texts() {
    let repository = support::repository_root();
    let canonical = [("LICENSE-MIT", "MIT License"), ("LICENSE-APACHE", "Apache License")];
    for (name, marker) in canonical {
        assert!(support::read(repository.join(name)).contains(marker), "{name} must contain {marker}");
    }

    let metadata = support::metadata();
    assert!(!metadata.workspace_packages().is_empty(), "lane metadata must contain packages");
    for package in metadata.workspace_packages() {
        let license =
            package.license.as_deref().unwrap_or_else(|| panic!("{} must declare a license", package.name));
        assert_eq!(license, "MIT OR Apache-2.0", "{} has an unexpected license", package.name);
        let directory =
            package.manifest_path.parent().expect("package manifest must have a parent").as_std_path();

        for (name, _) in canonical {
            let package_license = directory.join(name);
            assert!(package_license.is_file(), "{} must carry {name}", package.name);
            assert_eq!(
                support::read(package_license),
                support::read(repository.join(name)),
                "{}/{} must match the repository copy",
                package.name,
                name
            );
        }

        let manifest = support::manifest(package.manifest_path.as_std_path());
        if let Some(include) = manifest["package"].get("include").and_then(toml::Value::as_array) {
            assert!(
                include.iter().filter_map(toml::Value::as_str).any(|entry| entry.contains("LICENSE")),
                "{} uses package.include but omits license files",
                package.name
            );
        }
    }
}

#[test]
fn extension_dependency_is_registry_resolvable() {
    let metadata = support::metadata();
    let package = metadata
        .packages
        .iter()
        .find(|package| package.name == "kamu-money-pg")
        .expect("metadata must contain kamu-money-pg");
    let dependency = package
        .dependencies
        .iter()
        .find(|dependency| dependency.name == "kamu-money-core" && dependency.kind == DependencyKind::Normal)
        .expect("kamu-money-pg must depend on kamu-money-core");

    assert_ne!(dependency.req.to_string(), "*", "money-core needs a version requirement");
    assert!(
        dependency.path.is_none(),
        "money-core must not carry a manifest path; local tests inject a Cargo patch"
    );
    assert_eq!(package.publish.as_deref(), Some(&[][..]), "the extension lane must remain publish = false");
}

#[test]
fn control_file_matches_the_sql_extension_identity() {
    let path = support::lane_root().join("kamu-money-pg/kmoney.control");
    let control = support::read(&path);
    assert!(!control.contains("kmoney_t"), "control file must name the current kmoney type");
    assert_eq!(
        path.file_stem().and_then(|stem| stem.to_str()),
        Some("kmoney"),
        "the control-file stem is the CREATE EXTENSION name"
    );
}

#[test]
fn boundary_probe_cannot_enter_a_deployable_artifact() {
    let root = support::lane_root();
    let metadata = support::metadata();
    let package = metadata
        .packages
        .iter()
        .find(|package| package.name == "kamu-money-pg")
        .expect("metadata must contain kamu-money-pg");
    assert!(
        !package.features["default"].iter().any(|feature| feature == "boundary-probe"),
        "boundary-probe must not be a default feature"
    );
    assert!(package.features["boundary-probe"].is_empty(), "boundary-probe must remain a leaf feature");

    let dockerfile = support::read(root.join("kamu-money-pg/yb/Dockerfile"));
    let extra_features = dockerfile
        .lines()
        .find(|line| line.trim_start().starts_with("ARG EXTRA_FEATURES"))
        .expect("YugabyteDB Dockerfile must declare EXTRA_FEATURES");
    assert_eq!(extra_features.trim(), "ARG EXTRA_FEATURES=");
    assert!(
        dockerfile.contains("FROM node AS boundary-node"),
        "benchmark probe must use a separate Docker target"
    );
    assert!(
        !support::read(root.join("kamu-money-pg/yb/node-image.sh")).contains("boundary"),
        "deployable node builder must not mention the benchmark probe"
    );

    let release = support::recipe_body(&support::just_dump(&root), "gate-pg-release");
    assert!(release.contains("rs_noop"), "release proof must inspect shipped bytes for benchmark symbols");
}

#[test]
fn public_docs_do_not_call_the_scalar_a_store() {
    let root = support::lane_root();
    let mut offenders = Vec::new();
    for relative in support::tracked_files(None).into_iter().filter(|path| {
        path.extension().is_some_and(|extension| extension == "md")
            || path.extension().is_some_and(|extension| extension == "control")
    }) {
        let text = support::read(root.join(&relative));
        for (index, line) in text.lines().enumerate() {
            if ["OLTP store", "wallet store"].iter().any(|phrase| line.contains(phrase)) {
                offenders.push(format!("{}:{}: {}", relative.display(), index + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "kmoney is an amount scalar, not an application store:\n{}",
        offenders.join("\n")
    );
}
