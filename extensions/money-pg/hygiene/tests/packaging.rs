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
        // `docker buildx build` is a build too. Matching only the bare form would let a buildx
        // invocation resolve kamu-money-core differently from its siblings and say nothing,
        // which is the failure this guard exists to make impossible.
        let source = support::read(root.join(caller));
        let builds: Vec<&str> = source
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with('#')
                    && (trimmed.contains("docker build") || trimmed.contains("docker buildx build"))
            })
            .collect();

        // A positive control. Composing the command in one place and the context in another --
        // `cmd=(docker build)` here, `"${cmd[@]}" "${ARGS[@]}"` there -- leaves nothing for the
        // loop below to inspect, and that silence reads exactly like compliance.
        assert!(
            !builds.is_empty(),
            "{caller} no longer names a Docker build on any line, so this guard would pass \
             vacuously; keep the command and the core context together, or re-point the guard"
        );

        for line in builds {
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

/// Split a Dockerfile into its RUN instructions, one entry per instruction.
///
/// Backslash continuations and `<<'HEREDOC'` bodies both belong to the instruction that opened
/// them, because BuildKit keys the layer on the whole expanded command. A guard reading single
/// lines could see a build argument's expansion and the compile it is meant to scope as unrelated,
/// and would then pass on a Dockerfile where they had drifted into different instructions.
fn run_instructions(dockerfile: &str) -> Vec<String> {
    let lines: Vec<&str> = dockerfile.lines().collect();
    let mut instructions = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        index += 1;
        if !line.trim_start().starts_with("RUN ") {
            continue;
        }

        let mut instruction = line.to_owned();
        let heredoc = line
            .split_once("<<")
            .map(|(_, rest)| {
                rest.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_matches(['\'', '"', '-'])
                    .to_owned()
            })
            .filter(|delimiter| !delimiter.is_empty());

        match heredoc {
            Some(delimiter) => {
                while index < lines.len() {
                    let next = lines[index];
                    index += 1;
                    instruction.push('\n');
                    instruction.push_str(next);
                    if next.trim() == delimiter {
                        break;
                    }
                }
            }
            None => {
                while instruction.trim_end().ends_with('\\') && index < lines.len() {
                    instruction.push('\n');
                    instruction.push_str(lines[index]);
                    index += 1;
                }
            }
        }
        instructions.push(instruction);
    }
    instructions
}

fn scopes_the_dependency_compile(dockerfile: &str) -> bool {
    run_instructions(dockerfile)
        .iter()
        .any(|run| run.contains("cargo build --release") && run.contains("${KMONEY_CACHE_ID}"))
}

fn mounts_a_cache(dockerfile: &str) -> bool {
    run_instructions(dockerfile).iter().any(|run| run.contains("--mount=type=cache"))
}

/// The YugabyteDB dependency compile must be scoped by `KMONEY_CACHE_ID`, EXPANDED inside the
/// instruction that performs it.
///
/// `gate-pg-release` derives a value per run that no other run has used. While the compile lived
/// in a BuildKit cache mount, that made the mount empty by definition. It now busts the dependency
/// LAYER, by the same mechanism and for the same reason: BuildKit keys a RUN on its expanded
/// command, so an unseen value is a guaranteed miss and therefore a genuine from-scratch build.
///
/// What this exists to make impossible is an ARG declared and never referenced. It scopes nothing,
/// changes no cache key, and reads exactly like a build argument that works -- so the release proof
/// would go on claiming a from-scratch compile while assembling one out of whatever the daemon had.
#[test]
fn the_release_proof_compiles_the_yb_dependencies_from_empty() {
    let root = support::lane_root();
    let dockerfile = support::read(root.join("kamu-money-pg/yb/Dockerfile"));

    assert!(
        dockerfile.lines().any(|line| line.trim() == "ARG KMONEY_CACHE_ID=shared"),
        "the YugabyteDB image must declare KMONEY_CACHE_ID, defaulting to the shared scope"
    );
    assert!(
        scopes_the_dependency_compile(&dockerfile),
        "the YugabyteDB dependency compile must expand ${{KMONEY_CACHE_ID}} in the SAME RUN that \
         runs `cargo build --release`; a declaration elsewhere busts no layer"
    );

    // Controls. Without them this guard would pass on all three of the shapes it exists to reject.
    assert!(
        !scopes_the_dependency_compile(
            "ARG KMONEY_CACHE_ID=shared\nRUN cargo build --release -p kamu-money-pg\n"
        ),
        "a declared-but-unreferenced ARG scopes nothing and must not satisfy this guard"
    );
    assert!(
        !scopes_the_dependency_compile(
            "RUN echo ${KMONEY_CACHE_ID}\nRUN cargo build --release -p kamu-money-pg\n"
        ),
        "the reference must share an instruction with the compile, not merely the file"
    );
    assert!(
        scopes_the_dependency_compile(
            "RUN <<'SETUP' bash -e\n  echo ${KMONEY_CACHE_ID}\n  cargo build --release -p x\nSETUP\n"
        ),
        "a heredoc body belongs to the instruction that opened it"
    );

    // A cache mount at /work/target would MASK the layer beneath it, so the two cannot coexist:
    // reintroducing one would leave the dependency layer built, exported, restored -- and unused.
    //
    // Read from the RUN instructions, not the file: the comments there NAME the mechanism they
    // replaced, and prose explaining why something is absent must not read as its presence.
    assert!(
        !mounts_a_cache(&dockerfile),
        "the YugabyteDB image compiles into layers; a cache mount would shadow them and is also \
         builder-local, which is what made this compile unreachable by any CI cache"
    );
    assert!(
        mounts_a_cache("RUN --mount=type=cache,target=/work/target cargo build --release\n"),
        "the mount check must still see a mount that is actually there"
    );

    let release = support::recipe_body(&support::just_dump(&root), "gate-pg-release");
    assert!(
        release.contains("export KMONEY_CACHE_ID=") && release.contains("/dev/urandom"),
        "gate-pg-release must DERIVE a scope no earlier build can have filled, not name one"
    );
}

/// Every YugabyteDB build whose bytes ship carries the release scope.
///
/// `artifact` is what the suites certify and `node` is what an orchestrator deploys; a release
/// build of either that silently reused a shared layer would be exactly the claim
/// `gate-pg-release` exists to refuse. `boundary-node` is a measurement artefact that never
/// reaches a deployable image, and `deps` only warms the cache, so neither is required to carry it.
#[test]
fn shipped_yugabytedb_builds_carry_the_release_scope() {
    let root = support::lane_root();
    let mut builds = Vec::new();
    for caller in ["Justfile", "kamu-money-pg/yb/node-image.sh", "kamu-money-pg/bench/run-bench-boundary-yb.sh"]
    {
        let source = support::read(root.join(caller));
        for (line, logical) in support::logical_lines(&source) {
            // A trailing space excludes `Dockerfile.pg15`, which builds the A/B reference.
            if !logical.trim_start().starts_with('#') && logical.contains("kamu-money-pg/yb/Dockerfile ")
            {
                builds.push((caller, line, logical));
            }
        }
    }

    // A positive control. Routing these builds through a helper would leave nothing to inspect,
    // and that silence reads exactly like compliance.
    assert!(
        !builds.is_empty(),
        "no caller names a kamu-money-pg/yb/Dockerfile build any more, so this guard would pass \
         vacuously; keep the build and its scope together, or re-point the guard"
    );

    for (caller, line, logical) in &builds {
        let ships = ["--target artifact", "--target node"].iter().any(|target| logical.contains(target));
        if ships {
            assert!(
                logical.contains("--build-arg KMONEY_CACHE_ID"),
                "{caller}:{line} builds shipped YugabyteDB bytes without passing KMONEY_CACHE_ID, \
                 so gate-pg-release could not force it to compile from empty: {logical}"
            );
        }
    }
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
