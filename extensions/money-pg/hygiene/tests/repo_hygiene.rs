//! Facts about the repository that no compiler checks, and that went stale in silence.
//!
//! Every assertion here corresponds to a defect an external review found in `09a121f`. They
//! are not style preferences — each one was wrong in the tree, and wrong in a way that the
//! full test gate reported as green.
//!
//! These read files outside the crate, so a packaged `.crate` has no repository to inspect.
//! They skip rather than fail there: the point is to guard the working tree, and a test that
//! fails on a published artifact would be a worse defect than the ones it prevents.

use std::path::{Path, PathBuf};

/// The lane's workspace root.
///
/// THIS PANICS WHERE IT USED TO RETURN `None`, and that is the point of moving it here. The
/// original lived in a PUBLISHED crate, so it had to tolerate running from an unpacked `.crate`
/// with no repository around it -- every test opened with `let Some(root) = repo_root() else {
/// return; };` and passed, silently, having inspected nothing.
///
/// This crate is `publish = false` and inherits that from the lane root. There is no `.crate` for
/// it to be unpacked from, so the tolerance has no case left to cover and only the failure mode
/// remains: a root-discovery change would turn all sixteen guards green while they read no files.
/// Plan A found exactly that shape already live in `src/stable_hash.rs` after a move.
///
/// `Justfile` is the marker. It sits at the lane root and never inside a package directory.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent -- the crate moved out of the lane")
        .to_path_buf();
    assert!(
        root.join("Justfile").is_file(),
        "no Justfile at {} -- root discovery is broken, and every guard in this file would \
         otherwise pass while reading nothing",
        root.display()
    );
    root
}

/// The REPOSITORY root, one level above the lane.
///
/// Needed because some policy this lane depends on is enforced up there now: `scrub` and
/// `lint-shell` are the root's recipes and cover the whole tree including this directory. A guard
/// that asserted the lane's own gate ran them would be asserting something no longer true, and
/// deleting the guard instead would drop the only check that the scanner is invoked at all.
///
/// Identified by `crates/`, which exists at the repository root and not at the lane root -- so
/// this cannot silently resolve to the same directory `repo_root()` returns.
fn repository_top() -> PathBuf {
    let top = repo_root()
        .parent()
        .expect("the lane root has no parent")
        .parent()
        .expect("extensions/ has no parent")
        .to_path_buf();
    assert!(
        top.join("crates").is_dir() && top.join("Justfile").is_file(),
        "{} is not the repository root -- expected both crates/ and a Justfile",
        top.display()
    );
    top
}

/// Every workspace member's `Cargo.toml` declares `license = "MIT"`, and for a long time the
/// repository contained no licence text at all. A licence field without a licence is a claim,
/// not a grant.
///
/// THE MEMBER LIST IS READ, NOT WRITTEN. It used to say `["kamu-money-core", "kamu-money-pg"]`, which is
/// how a test named for protecting the published packages stayed green while `kamu-money-iso` joined
/// the workspace and shipped with no licence at all. A test that names its subject only checks
/// the subjects someone remembered to name; deriving it from the manifest means the next member
/// is covered on the day it is added rather than the day someone notices.
#[test]
fn the_licence_text_reaches_every_published_package() {
    let root = repo_root();
    let top = repository_top();

    // TWO TEXTS, NOT ONE, AND THEY LIVE AT THE REPOSITORY ROOT. The source repository was
    // MIT-only with a single LICENSE at its workspace root; this one is dual-licensed
    // `MIT OR Apache-2.0` and keeps both texts up top, which is what the lane's
    // [workspace.package] license field now inherits from.
    let canonical: Vec<(&str, &str)> =
        vec![("LICENSE-MIT", "MIT License"), ("LICENSE-APACHE", "Apache License")];
    for (name, marker) in &canonical {
        let text = std::fs::read_to_string(top.join(name))
            .unwrap_or_else(|e| panic!("the repository root has no readable {name}: {e}"));
        assert!(text.contains(marker), "{name} must be the {marker} text");
    }

    let members = workspace_members(&root);
    // A POSITIVE CONTROL, NOT A MAGIC NUMBER. The original required `>= 3` because the workspace
    // had three members and a shrinking list was the failure it feared. This one has two, and a
    // hardcoded count would have to be edited every time the lane gains a crate -- which makes it
    // a number people update to make a test pass. What actually needs guarding is that the parser
    // returned anything at all: if the members list moves or its shape changes, the loop below
    // iterates zero times and this test passes having checked no package whatsoever.
    assert!(
        !members.is_empty(),
        "no workspace members were parsed -- the members list moved or the parser broke, and \
         this test was about to pass without checking any package"
    );

    for member in &members {
        let manifest = std::fs::read_to_string(root.join(member).join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("{member}/Cargo.toml is readable: {e}"));
        if !manifest.contains("license") {
            continue; // a member that claims nothing has nothing to honour
        }

        // EACH PACKAGE CARRIES BOTH TEXTS AS REAL FILES, byte-identical to the root's. That is
        // this repository's convention -- every one of the nine published crates does it -- and
        // it differs from the source's, which symlinked one MIT text into each member so that a
        // single file served them all.
        //
        // The symlink assertion did not travel, because the mechanism was never the point: the
        // property is that the texts CANNOT DRIFT. Comparing contents asserts that directly, and
        // asserts it whether the file is a link, a copy, or something a future tool materialises.
        // The source's own comment records a `perl -pi` sweep that silently turned all three links
        // into copies and broke nothing, precisely because the copies still agreed -- content
        // equality is the check that would have stayed meaningful through that.
        //
        // `cargo package` does not descend to the workspace root from a member, so a package whose
        // licence text lives only up top ships a claim with no grant.
        for (name, _) in &canonical {
            let member_licence = root.join(member).join(name);
            assert!(
                member_licence.is_file(),
                "{member} declares a license but does not carry {name}, so its package would \
                 ship a claim with no grant"
            );
            let theirs = std::fs::read_to_string(&member_licence)
                .unwrap_or_else(|e| panic!("{member}/{name} is readable: {e}"));
            let ours = std::fs::read_to_string(top.join(name)).expect("root licence is readable");
            assert_eq!(
                theirs, ours,
                "{member}/{name} has drifted from the repository root's copy. Two licence texts \
                 that disagree is the failure this checks for; restore with: \
                 cp {name} extensions/money-pg/{member}/{name}"
            );
        }

        // An explicit `include` is a WHITELIST, so a package can carry the file on disk and
        // still omit it from the archive. This is exactly how kamu-money-iso shipped without one.
        if let Some(include) = manifest.lines().find(|l| l.trim_start().starts_with("include")) {
            assert!(
                include.contains("LICENSE"),
                "{member} has an explicit `include` that omits LICENSE, so the file exists on \
                 disk and never reaches the package: {include}"
            );
        }
    }
}

/// Workspace member directories, read from the root manifest.
///
/// A deliberately small parser rather than a TOML dependency: it reads one known key in one
/// known file, and a dev-dependency that exists to check a licence claim is its own kind of
/// overreach. It fails loudly if the shape it expects is gone.
fn workspace_members(root: &Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("root manifest");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("members"))
        .expect("root manifest declares workspace members");
    let list = line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(inner, _)| inner)
        .expect("members is a single-line array; update this parser if it wrapped");
    list.split(',').map(|s| s.trim().trim_matches('"').to_owned()).filter(|s| !s.is_empty()).collect()
}

/// `kamu-money-pg` depended on `kamu-money-core` by path alone, which makes the crate impossible to
/// package: cargo refuses with "all dependencies must have a version requirement specified".
///
/// This asserts the requirement exists, NOT that `cargo package -p kamu-money-pg` succeeds. It
/// cannot succeed until `kamu-money-core` is actually on crates.io — that residual failure is
/// publication ORDER, not a defect, and asserting it would pin a fact that is meant to change.
#[test]
fn the_pgrx_crate_can_be_packaged() {
    let root = repo_root();
    let manifest =
        std::fs::read_to_string(root.join("kamu-money-pg/Cargo.toml")).expect("manifest is readable");
    let dep = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("kamu-money-core"))
        .expect("kamu-money-pg depends on kamu-money-core");

    // THE SHAPE CHANGED, SO THE ASSERTION DID. The original read `{ version = "0.1.0", path = ".." }`
    // and checked for the literal `version` key, because a path-only dependency makes the crate
    // impossible to package -- cargo refuses with "all dependencies must have a version
    // requirement specified". After the re-home there is NO path at all: the dependency is a plain
    // `kamu-money-core = "0.1"`, and the path is injected on the command line by the lane's
    // recipes so that no manifest is ever rewritten.
    //
    // That satisfies the same requirement more strongly -- there is nothing left that could be
    // path-only -- but it has no literal `version` key, so the old check failed on the better
    // shape. What matters is that a version REQUIREMENT is present in either form, and that no
    // path has crept back in.
    let has_requirement = dep.contains("version") || dep.split('=').nth(1).is_some_and(|v| v.contains('"'));
    assert!(
        has_requirement,
        "kamu-money-core needs a version requirement, in either the plain or the table form, \
         or kamu-money-pg cannot be packaged: {dep}"
    );
    assert!(
        !dep.contains("path"),
        "kamu-money-core must NOT carry a path in the manifest -- the lane's recipes inject one \
         via `--config patch.crates-io...`, and a manifest that names a path is a manifest \
         something will eventually rewrite: {dep}"
    );
}

/// The `_t` suffix was removed from the SQL types, and this file kept describing the old
/// name. Nothing reads the comment, so nothing caught it — it survived the rename commit,
/// the rename's spec update, and a full four-major matrix run.
#[test]
fn the_control_file_does_not_describe_a_type_that_was_renamed() {
    let root = repo_root();
    let control =
        std::fs::read_to_string(root.join("kamu-money-pg/kmoney.control")).expect("control readable");
    assert!(
        !control.contains("kmoney_t"),
        "control file still names kmoney_t; the type is kmoney:\n{control}"
    );

    // The control file's STEM names the extension (cargo-pgrx `find_control_file`), so the
    // underscore here is load-bearing and must survive any crate rename.
    assert!(
        root.join("kamu-money-pg/kmoney.control").is_file(),
        "the control file must stay kmoney.control -- its stem IS the extension name, and \
         renaming it forces double-quoting in every CREATE/ALTER EXTENSION"
    );
}

/// `kmoney` is an amount SCALAR for OLTP wallet/ledger schemas — not a wallet, ledger, or store.
/// This workspace implements no account, transaction, journal or balance, so calling the type a
/// "store" claims application guarantees that do not exist here, and then leans on them to
/// justify the SQL surface.
///
/// This is a CLASS guard, and it exists because fixing the instances did not hold. An external
/// review named three sites by line; those three were corrected and six identical claims
/// survived elsewhere — including a dated Note in a design document still giving the retracted premise
/// as the *current* rationale, a thousand lines after §0 had dropped it. A grep is the only
/// thing that scales to "and everywhere else".
///
/// The limitation itself is unaffected and should still be stated wherever it is relevant: no
/// operator class means no sort operator, no value index, and no `GROUP BY`/`DISTINCT`/`UNIQUE`
/// by amount. That is a property of this scalar, true regardless of what the consumer's schema
/// does — which is exactly why it must not be phrased as one.
#[test]
fn no_tracked_file_calls_the_type_a_store() {
    let root = repo_root();
    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs");
    let mut offenders = Vec::new();
    for rel in listed.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let rel = String::from_utf8_lossy(rel).into_owned();
        // This test names the phrases, so it would otherwise match itself.
        if rel.ends_with("repo_hygiene.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue; // binary or unreadable: nothing to assert about
        };
        for (i, line) in text.lines().enumerate() {
            if line.contains("OLTP store") || line.contains("wallet store") {
                offenders.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`kmoney` is an amount scalar for OLTP wallet/ledger schemas, not a store — this \
         workspace has no account, transaction or balance. Say \"amount scalar for OLTP \
         wallet/ledger schemas\" and state the opclass limitation independently of any \
         assumption about the consuming schema:\n{}",
        offenders.join("\n")
    );
}

/// No script captures `docker exec` output with a HOST-side `2>&1`.
///
/// **`docker exec` carries stdout and stderr as two separately multiplexed channels**, so a
/// host-side `2>&1` merges streams whose relative order was already lost in transit. It can only
/// interleave whatever arrived, whenever it arrived — and the misordering is intermittent, which
/// is the worst possible shape for a gate: it goes green on the re-run, and the re-run teaches
/// everyone that red means "try again".
///
/// This is not hypothetical twice over. `run-yb-regress.sh` measured it putting expected-error
/// lines one `\echo` section late in 2 of 11 cases, and fixed itself. `run-yb.sh` kept the
/// host-side form and failed the 2026-07-25 release gate with one hunk: an expected ERROR and the
/// section marker after it had swapped. Both engines had independently passed all 20 oracle
/// assertions; the immediate re-run was byte-exact. One script learning the lesson did not stop
/// the other from paying for it, which is the argument for a guard rather than a third fix.
///
/// Merging INSIDE the container settles the order at the source:
/// `docker exec N bash -c 'exec cmd "$@" 2>&1' x <args>`.
///
/// `/dev/null` is exempt: a readiness probe that discards its output has no order to lose.
#[test]
fn no_script_merges_docker_exec_streams_on_the_host() {
    let root = repo_root();
    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z", "*.sh"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs");

    let mut offenders = Vec::new();
    for rel in listed.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let rel = String::from_utf8_lossy(rel).into_owned();
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };

        // LOGICAL lines. A per-physical-line scan is unsound here and quietly so: split the
        // redirect onto a continuation and `docker exec` and `2>&1` land on different lines, so
        // neither matches and the guard passes on the exact code it exists to reject.
        let mut logical = String::new();
        let mut start = 0usize;
        for (i, line) in text.lines().enumerate() {
            if logical.is_empty() {
                start = i + 1;
            }
            logical.push_str(line.strip_suffix('\\').unwrap_or(line));
            if line.ends_with('\\') {
                continue;
            }

            if merges_docker_exec_streams_on_the_host(&logical) {
                offenders.push(format!("{rel}:{start}: {}", logical.trim()));
            }
            logical.clear();
        }
    }

    assert!(
        offenders.is_empty(),
        "`docker exec` multiplexes stdout and stderr separately, so a host-side `2>&1` cannot \
         order them and the misordering is INTERMITTENT. Merge inside the container instead:\n  \
         docker exec N bash -c 'exec cmd \"$@\" 2>&1' x <args>\n\n{}",
        offenders.join("\n")
    );
}

/// `kmoney_allocate` takes a borrowed pgrx `Array`, so its size cap runs before any copy.
///
/// **A SOURCE guard, and it says so because it cannot be anything else.** The property is "pgrx
/// did not materialise the argument", which happens in generated wrapper code before the function
/// body exists — invisible from SQL, and identical in every observable behaviour to the version
/// that did. The signature is the only place the difference is written down, so the signature is
/// what is pinned.
///
/// What it prevents: `weights: Vec<Option<i32>>` looks like the obvious spelling, reads more
/// simply, and passes every existing test. It also means `impl FromDatum for Vec<Option<T>>` runs
/// `Array::from_polymorphic_datum(..).map(|a| a.iter().collect::<Vec<_>>())` first, so a hostile
/// `ARRAY[…]` is walked and copied into a Rust allocation *before* `MAX_ALLOCATE_PARTS` can reject
/// it. The 65 536 / 65 537 tests pin the semantic threshold and would pass unchanged with the cap
/// doing nothing for memory at all — which is exactly what an external review found on 2026-07-25.
///
/// Reverting the signature is therefore a silent regression, and this is the alarm on it.
#[test]
fn kmoney_allocate_does_not_materialize_its_weights_before_the_cap() {
    let root = repo_root();
    // SEARCHES THE WHOLE CRATE, not `lib.rs`. The 2026-07-27 module split moved this
    // function into `allocation.rs`, and this guard went red -- correctly, and for the wrong
    // reason. A guard that names one file stops guarding the moment the code it protects is
    // relocated, and the relocation is exactly when a `Vec<Option<..>>` signature could come
    // back unnoticed. Naming the crate instead of the file is what makes it survive the next
    // move as well as this one.
    let src = root.join("kamu-money-pg/src");
    let lib = std::fs::read_dir(&src)
        .expect("kamu-money-pg/src is readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .find(|t| t.lines().any(|l| l.trim_start().starts_with("fn kmoney_allocate(")))
        .expect("kamu-money-pg still defines kmoney_allocate somewhere under src/");

    let signature =
        lib.lines().find(|l| l.trim_start().starts_with("fn kmoney_allocate(")).expect("found above");

    assert!(
        signature.contains("Array<"),
        "kmoney_allocate must take a BORROWED pgrx `Array`, so `len()` can be read from the array \
         header before anything is copied. Found:\n  {}",
        signature.trim()
    );
    assert!(
        !signature.contains("Vec<Option<"),
        "kmoney_allocate takes `Vec<Option<..>>` again. pgrx collects that from the datum BEFORE \
         the body runs, so MAX_ALLOCATE_PARTS is enforced after the allocation it exists to \
         prevent — and every existing test still passes. Found:\n  {}",
        signature.trim()
    );

    // Order matters as much as the type: reading `len()` after the first `.iter()` would put the
    // per-element walk back in front of the cap without changing the signature.
    let body = &lib[lib.find("fn kmoney_allocate(").expect("found above")..];
    let body = &body[..body.find("\n}\n").map_or(body.len(), |e| e + 2)];
    let len_at = body.find(".len()");
    let iter_at = body.find(".iter()");
    assert!(
        matches!((len_at, iter_at), (Some(l), Some(i)) if l < i),
        "kmoney_allocate must read `.len()` BEFORE it iterates — the cap is only a resource bound \
         if nothing per-element has run yet (len at {len_at:?}, first iter at {iter_at:?})"
    );
}

/// No tracked file teaches `just <recipe> name=value`, because `just` has no such call syntax.
///
/// **This is a CLASS guard for a defect that shipped twice.** `just` binds recipe arguments
/// POSITIONALLY, so `just gate-pg-release jobs=3` passes the literal string `jobs=3` as the first
/// argument — and reads exactly like it worked. On 2026-07-25 it did precisely that in the release
/// gate: the numeric tests inside died with "integer expected", and a failed `[` inside an
/// `if`/`while` *condition* is exempt from `set -e`, so the recipe ran on with its throttle
/// silently disabled and exited 0. The recipes now validate their arguments, which closes the
/// runtime half. This closes the other half: four more examples of the invalid form survived that
/// fix, and two of them were inside the error message an operator reads while already diagnosing
/// an image-adoption failure — the worst possible place to hand someone a call that lies.
///
/// The search anchors on names parsed out of the `Justfile`, so it follows recipe renames and
/// cannot fire on `FOO=1 just x` (the assignment precedes `just`) or on prose that merely
/// contains `=`.
///
/// Deliberate anti-examples are allowed — the positional rule cannot be explained without showing
/// the broken form — but they must SAY they are anti-examples, with a `just-anti-example` marker
/// on the line or the one above it. An exemption you have to write down is one a reviewer can
/// see; a heuristic keyed on nearby prose would drift the first time someone reworded a paragraph.
#[test]
fn no_tracked_file_teaches_just_name_value_arguments() {
    const MARKER: &str = "just-anti-example";

    let root = repo_root();
    let justfile = std::fs::read_to_string(root.join("Justfile")).expect("Justfile readable");

    // Recipe headers start at column 0 with a lowercase letter and carry a `:`. The name is the
    // first token before that colon, so `check-pg pg="18":` and `check: fmt-check lint` both
    // yield the right thing.
    let recipes: Vec<&str> = justfile
        .lines()
        .filter(|l| l.starts_with(|c: char| c.is_ascii_lowercase()))
        .filter_map(|l| l.split(':').next())
        .filter_map(|head| head.split_whitespace().next())
        .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .collect();
    assert!(
        recipes.len() > 10,
        "parsed only {} recipes out of the Justfile — the parser is broken, and a broken parser \
         makes this guard vacuously true",
        recipes.len()
    );

    // `just yb-soak 30` is correct and `just yb-soak minutes=30` is not, so what makes a hit is an
    // `ident=` token anywhere in the arguments. Not just the recipe's REAL parameter names: a
    // misremembered one (`duration=30`) fails in exactly the same silent way, and the reader
    // learns the same wrong syntax from it.
    fn teaches_name_value(rest: &str) -> bool {
        rest.split_whitespace().any(|tok| {
            let tok = tok.trim_start_matches(['`', '\'', '"', '(']);
            let Some((name, _)) = tok.split_once('=') else {
                return false;
            };
            !name.is_empty()
                && name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
    }

    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs");
    let mut offenders = Vec::new();
    for rel in listed.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let rel = String::from_utf8_lossy(rel).into_owned();
        // This test spells out the broken form to explain itself, so it would match itself.
        if rel.ends_with("repo_hygiene.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue; // binary or unreadable: nothing to assert about
        };
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let exempt =
                line.contains(MARKER) || i.checked_sub(1).is_some_and(|prev| lines[prev].contains(MARKER));
            if exempt {
                continue;
            }
            for name in &recipes {
                let needle = format!("just {name} ");
                for (at, _) in line.match_indices(&needle) {
                    if teaches_name_value(&line[at + needle.len()..]) {
                        offenders.push(format!("{rel}:{}: {}", i + 1, line.trim()));
                    }
                }
            }
        }
    }
    offenders.dedup();
    assert!(
        offenders.is_empty(),
        "`just` arguments are POSITIONAL — `just <recipe> name=value` passes the literal string \
         \"name=value\" and reads as though it worked. Use `just yb-soak 30`, `just yb-ab \
         \"$TAG\"`. If the line is deliberately showing the broken form, put `{MARKER}` on it or \
         on the line above:\n{}",
        offenders.join("\n")
    );
}

/// Every `#[pg_test]` in `kamu-money-pg` is accounted for by the portable case suite.
///
/// **This is the guard that stops a skip from being counted as a pass.** `cargo pgrx test` manages
/// its own PostgreSQL and cannot be aimed at YugabyteDB, so the 54 tests that encode this type's
/// contract were restated as `sql/` + `expected/` pairs in `kamu-money-pg/tests/pg_regress`, which
/// run against any live server. The risk in a port is not that a case fails — a failing case is
/// loud. It is that a case is quietly never written, and the suite reports green over a hole.
///
/// So `COVERAGE.md` is a manifest, and this parses `lib.rs` to check it is complete. A test with
/// no row fails here. A row naming a case file that does not exist fails here. A row naming a test
/// that no longer exists fails here. A case that cannot be ported is allowed, but it has to say
/// `NOT-PORTABLE: <reason>` out loud.
///
/// Runs with no Docker and no database, so the check is in `just check` rather than behind a
/// container suite somebody might not run.
#[test]
fn the_case_suite_accounts_for_every_pg_test() {
    let root = repo_root();
    // EVERY source file in the crate, concatenated -- not `lib.rs`. This is the second guard in
    // this file to have been keyed to a path and broken by the 2026-07-27 module split;
    // the in-backend suite now lives beside the code it tests, in `ops.rs`, `wire.rs` and so on.
    // A guard scoped to one file stops guarding the moment its subject moves, which is precisely
    // when a test can go missing, so the scope is the crate and the ordering is deterministic.
    let mut sources: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("kamu-money-pg/src"))
        .expect("kamu-money-pg/src is readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    sources.sort();
    let lib = sources
        .iter()
        .map(|p| std::fs::read_to_string(p).expect("crate source is readable"))
        .collect::<Vec<_>>()
        .join("\n");

    // Line-scan rather than a syntax crate: the attribute has two spellings (`#[pg_test]` and a
    // multi-line `#[pg_test(error = "...")]`), and in both the *next* line starting with `fn ` is
    // the test. Doc comments and ordinary comments precede the attribute, never separate it from
    // the signature, and no line inside an `error = "..."` literal begins with `fn `.
    let mut tests = Vec::new();
    let mut pending = false;
    for line in lib.lines() {
        let t = line.trim_start();
        if t.starts_with("#[pg_test") {
            pending = true;
        } else if pending && t.starts_with("fn ") {
            let name = t.trim_start_matches("fn ").split('(').next().expect("a fn line has a name").trim();
            tests.push(name.to_owned());
            pending = false;
        }
    }
    assert!(
        !pending,
        "a #[pg_test] attribute was not followed by a `fn` — the parser above needs updating, and until then it is silently losing tests"
    );
    // A parser that finds nothing would make every assertion below vacuously true. The floor is
    // deliberately below the current count so adding a test is not a chore, but far enough above
    // zero that a broken parser cannot pass.
    assert!(
        tests.len() >= 50,
        "found only {} #[pg_test]s across kamu-money-pg/src — the parser is broken, not the crate",
        tests.len()
    );

    let suite = root.join("kamu-money-pg/tests/pg_regress");
    let coverage = std::fs::read_to_string(suite.join("COVERAGE.md"))
        .expect("kamu-money-pg/tests/pg_regress/COVERAGE.md is readable");

    // Every test names itself in the manifest, and its row names a case that exists.
    let mut missing = Vec::new();
    let mut bad_case = Vec::new();
    for name in &tests {
        let needle = format!("`{name}`");
        let Some(row) = coverage.lines().find(|l| l.starts_with('|') && l.contains(&needle)) else {
            missing.push(name.clone());
            continue;
        };
        if row.contains("NOT-PORTABLE:") {
            continue; // declared unportable, with its reason, which is the honest option
        }
        // The case column is the backticked name between the test and the description.
        let case = row
            .split('|')
            .map(str::trim)
            .find(|c| c.starts_with('`') && c.ends_with('`') && c.contains('-') && !c.contains(name.as_str()))
            .map(|c| c.trim_matches('`').to_owned());
        match case {
            Some(c)
                if suite.join(format!("sql/{c}.sql")).is_file()
                    && suite.join(format!("expected/{c}.out")).is_file() => {}
            Some(c) => bad_case.push(format!("{name} -> {c} (sql/ or expected/ file missing)")),
            None => bad_case.push(format!("{name} -> no case named in its row")),
        }
    }
    assert!(
        missing.is_empty(),
        "{} #[pg_test](s) have no row in kamu-money-pg/tests/pg_regress/COVERAGE.md. A ported \
         suite that silently omits a case reports green over a hole — give each a row, or a \
         `NOT-PORTABLE: <reason>`:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
    assert!(
        bad_case.is_empty(),
        "COVERAGE.md rows point at cases that do not exist:\n  {}",
        bad_case.join("\n  ")
    );

    // And the other direction: a row naming a test that was renamed or deleted. Without this the
    // manifest rots into a list of things that used to be true.
    let stale: Vec<String> = coverage
        .lines()
        .filter(|l| l.starts_with('|'))
        // Numbered data rows only. The header's own second column is the literal `#[pg_test]`,
        // which is not a test name and would otherwise be reported as the first thing to have
        // gone stale.
        .filter(|l| l.split('|').nth(1).is_some_and(|n| n.trim().parse::<u32>().is_ok()))
        .filter_map(|l| l.split('|').nth(2).map(str::trim))
        .filter(|c| c.starts_with('`') && c.ends_with('`'))
        .map(|c| c.trim_matches('`').to_owned())
        .filter(|c| !tests.contains(c))
        .collect();
    assert!(
        stale.is_empty(),
        "COVERAGE.md names #[pg_test]s that no longer exist anywhere in kamu-money-pg/src:\n  {}",
        stale.join("\n  ")
    );

    // THE GOLDENS THEMSELVES MUST NAME THE TESTS, not only the manifest.
    //
    // COVERAGE.md proves a row exists for every test. It cannot prove the case FILE actually
    // asserts that test, and the label printed into each golden (`-- <fn name>`) is the only thing
    // that ties a block of expected output back to the `#[pg_test]` it restates. A typo there
    // leaves the manifest green while the traceability it documents is broken -- and the next
    // person to change an assertion has no way to find the case that pins it.
    //
    // So the two sets must be equal in BOTH directions: no label that is not a test, and no test
    // without a label. Verified 1:1 across all 54 when this was written.
    let mut labels: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(suite.join("expected")).expect("expected/ is readable") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("out") {
            continue;
        }
        let golden = std::fs::read_to_string(&path).expect("golden is readable");
        labels.extend(golden.lines().filter_map(|l| l.strip_prefix("-- ")).map(|l| l.trim().to_owned()));
    }
    let unknown: Vec<&String> = labels.iter().filter(|l| !tests.contains(l)).collect();
    assert!(
        unknown.is_empty(),
        "golden files carry `-- <label>` lines that are not #[pg_test] names — the label IS the \
         traceability, so a typo there silently detaches a block of expected output from the test \
         it restates:\n  {unknown:?}"
    );
    // A test declared NOT-PORTABLE has no case, so it can have no label -- demanding one would
    // make the escape hatch unusable and push the honest answer ("this cannot be expressed in the
    // portable suite, and here is why") back into silent omission, which is the failure the whole
    // manifest exists to prevent. The row still has to SAY it, and the reason is still read by a
    // human; what is dropped is only the label requirement.
    let not_portable: Vec<String> = coverage
        .lines()
        .filter(|l| l.starts_with('|') && l.contains("NOT-PORTABLE:"))
        .filter_map(|l| l.split('|').nth(2).map(str::trim))
        .filter(|c| c.starts_with('`') && c.ends_with('`'))
        .map(|c| c.trim_matches('`').to_owned())
        .collect();
    let unlabelled: Vec<&String> =
        tests.iter().filter(|t| !labels.contains(t) && !not_portable.contains(t)).collect();
    assert!(
        unlabelled.is_empty(),
        "these #[pg_test]s have a COVERAGE.md row but no `-- <name>` label in any golden, so \
         nothing in the suite's own output says which case pins them (declare NOT-PORTABLE with a \
         reason if one genuinely cannot be restated portably):\n  {unlabelled:?}"
    );

    // No orphan case files. A case in sql/ that no row mentions is either coverage nobody recorded
    // or a leftover, and both are worth one line to say which.
    let mut orphans = Vec::new();
    for entry in std::fs::read_dir(suite.join("sql")).expect("sql/ is readable") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue; // .setup.sh companions are not cases
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).expect("utf8 stem").to_owned();
        if !coverage.contains(&format!("`{stem}`")) {
            orphans.push(stem);
        }
    }
    assert!(orphans.is_empty(), "case files with no row in COVERAGE.md: {}", orphans.join(", "));
}

/// `doc-pg` was a recipe beside the gate rather than inside it, and the result was a green
/// `gate-pg` next to a red `just doc-pg` — two rustdoc link errors that would have broken
/// the docs.rs build of a crate the gate had just called releasable.
#[test]
fn the_gates_include_the_documentation_build() {
    let root = repo_root();
    let justfile = std::fs::read_to_string(root.join("Justfile")).expect("Justfile is readable");

    for gate in ["gate-offline:", "gate-pg:"] {
        let line = justfile
            .lines()
            .find(|l| l.starts_with(gate))
            .unwrap_or_else(|| panic!("Justfile has no `{gate}` recipe"));
        assert!(
            line.contains("doc-pg"),
            "`{gate}` must depend on `doc-pg`; a check nobody runs is a check that does not \
             exist: {line}"
        );
    }

    // The same failure one level up. `gate-pg-release` is the recipe that decides whether a revision
    // may carry money on YugabyteDB, and each of these answers a question none of the others do:
    // byte-exactness against stock PG15 (yb-ab), the whole #[pg_test] contract on YB at all
    // (test-yb-regress), every node of a cluster (test-yb-cluster), conservation when transactions
    // overlap (test-yb-concurrent), and a read replica (test-yb-readreplica). Dropping one for
    // wall-clock is the obvious temptation and the exact thing that would make the gate's claim
    // untrue, quietly.
    // The whole RECIPE, not just its header line. `gate-pg-release` stopped being a dependency list
    // when it gained `jobs=` -- `just` has no `-j`, and a dependency list cannot express "build the
    // artifact once, then run the three suites that consume it concurrently", nor resolve the image
    // identity once and hand the same one to every stage. The suites are invoked from the body now,
    // so scanning only the header would have made this guard vacuously true: it would have found no
    // suite names and asserted nothing, which is the failure mode it exists to prevent one level up.
    let release = executable_recipe_body(&justfile, "gate-pg-release");
    assert!(!release.is_empty(), "Justfile has a `gate-pg-release` recipe");
    // The suites must be RUN, not merely named in a comment explaining why they matter.
    for required in [
        "just gate-pg",
        // `_yb-ab-ref`, the variant that takes an ALREADY-RESOLVED identity. See the negative
        // half of this assertion below for why the public `yb-ab` is not acceptable here.
        "just _yb-ab-ref",
        "run-yb-regress.sh",
        "run-yb-cluster.sh",
        "run-yb-concurrent.sh",
        // Read replicas are tservers: they run YSQL backends and need the library on their own
        // filesystem, while `CREATE EXTENSION` reaches them for free through the catalog. Every
        // other suite here uses primary nodes only, so dropping this one would take a whole class
        // of production node back out of the evidence without anything noticing.
        "run-yb-readreplica.sh",
        // Restore. The adoption contract deferred this "until a version migration"; a review
        // disagreed and was right -- a rolling VERSION upgrade can wait for a migration to be
        // planned, but restore is needed the moment the first production value exists. A dump
        // represents an extension as `CREATE EXTENSION` and does not dump its member objects, so
        // whether a clean cluster can execute that statement is a property of THIS extension, not
        // of the operator's backup tooling.
        "run-yb-restore.sh",
    ] {
        assert!(
            release.contains(required),
            "`gate-pg-release` must run `{required}` -- it is the exit criterion for one item of the \
             YugabyteDB production-readiness plan, and a gate that omits it claims more than it \
             checked:\n{release}"
        );
    }

    // AND IT MUST NOT RE-RESOLVE THE IMAGE. `gate-pg-release` resolves `$YB_REF` once precisely so
    // that the artifact build, the PG15 reference and all four suites are a claim about ONE
    // image. It then called `just yb-ab {{ tag }}` -- handing the MUTABLE TAG back to a recipe
    // that resolved it again, straight past the rule the lines above it explain. Both printed,
    // both agreed, and nothing was wrong that day; on a shared daemon, or with YB_PULL=1 while a
    // tag moves, the A/B and the suites would name different images and the run would still read
    // as green. A comment asserting single resolution is not single resolution.
    assert!(
        !release.contains("just yb-ab"),
        "`gate-pg-release` must call `just _yb-ab-ref \"$YB_REF\"` with the identity it already \
         resolved, never `just yb-ab <tag>` -- that resolves a second time, and the whole point \
         of resolving once is that the run is a claim about one image:\n{release}"
    );

    // AND IT MUST CERTIFY THE ARTIFACT THAT SHIPS. `gate-pg-release` used to boot the STOCK base
    // image and `docker cp` loose files in, while the Justfile, RUNBOOK.md §5 and the adoption
    // contract all named the node image as the deployable artifact. The gate therefore certified
    // a live-install harness and the contract promoted a different image, whose boot, paths,
    // permissions and catalog creation nothing had exercised. Both halves are required: building
    // the node image without `YB_REQUIRE_BAKED=1` leaves `docker cp` as a silent fallback, so a
    // node image that failed to deliver the extension would be rescued by the harness and the run
    // would still be green -- evidence bound to an artifact that was never run.
    for required in ["node-image.sh", "YB_REQUIRE_BAKED=1"] {
        assert!(
            release.contains(required),
            "`gate-pg-release` must contain `{required}`: the suites have to boot the DEPLOYABLE \
             node image, with installing-onto-a-running-node ruled out rather than merely \
             unused. Otherwise the release evidence names an image the run never exercised:\
             \n{release}"
        );
    }

    // The suite runner's own negative control is offline, so it belongs in the cheap gate. If it
    // only ran behind a container suite, the one thing that proves the YugabyteDB oracle still
    // rejects a wrong answer would be the first thing skipped on a slow machine.
    let check = justfile
        .lines()
        .find(|l| l.starts_with("gate-offline:"))
        .expect("Justfile has a `gate-offline` recipe");
    assert!(
        check.contains("test-regress-selftest"),
        "`gate-offline` must depend on `test-regress-selftest`; it needs no database, and it is what \
         proves the case-suite oracle has not rotted into always-passing: {check}"
    );

    // AND `scrub` MUST BE GATED. It greps the tracked tree for host paths, emails, private
    // addresses, credential prefixes, CPU models and the running user's own name -- and until
    // 2026-07-26 it was in no gate at all, so it had never once failed a build. It was also red
    // at that moment, on a false positive of its own. A scanner nobody runs is not a scanner,
    // and this workspace is being re-homed into a monorepo where the tree is the only thing that
    // travels.
    // IT IS THE ROOT'S GATE THAT MUST INVOKE IT NOW. `scrub` moved up when the lane was re-homed:
    // one implementation covers the whole tree, this directory included, because two copies of a
    // scanning policy drift until the forgotten one stops matching. So the assertion follows the
    // recipe rather than being deleted with it -- deleting it would have removed the only check
    // that the scanner is invoked by anything at all, which is the exact state that let it sit in
    // no gate until 2026-07-26, never once having failed a build.
    let top_justfile = std::fs::read_to_string(repository_top().join("Justfile"))
        .expect("the repository root has no readable Justfile");
    let lint_all = top_justfile
        .lines()
        .find(|l| l.starts_with("lint-all:"))
        .expect("the repository root Justfile has no `lint-all` recipe");
    assert!(
        lint_all.contains("scrub"),
        "the repository root's `lint-all` must depend on `scrub`, and `gate` must run `lint-all` \
         -- the tree is what survives, and a PII scanner that no gate invokes has never proven \
         anything: {lint_all}"
    );
    assert!(
        top_justfile.contains("\"just lint-all\""),
        "the repository root's `gate` must run `just lint-all`, or the scrub above is gated by a \
         recipe nothing calls"
    );
}

/// Everything outside single- and double-quoted spans.
///
/// The discriminator for the guard below: a `2>&1` *inside* the quoted argument of `bash -c` runs
/// in the container and is the fix; the same characters *after* the closing quote run on the host
/// and are the defect.
fn outside_quotes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match quote {
            None => match c {
                '\'' | '"' => quote = Some(c),
                _ => out.push(c),
            },
            // Inside DOUBLE quotes a backslash escapes the next character, so `\"` does not close
            // the span. Inside SINGLE quotes the shell has no escapes at all — which is exactly
            // why the in-container form is written with single quotes.
            Some('"') if c == '\\' => {
                chars.next();
            }
            Some(q) if c == q => quote = None,
            Some(_) => {}
        }
    }
    out
}

/// Does this logical shell line merge `docker exec`'s two streams **on the host**?
///
/// Extracted so it can be driven with fixtures. It used to exempt any line containing `bash -c`,
/// anywhere — a whole-line exemption for what is really a question about *where* the redirect
/// sits. So the host-side form
///
/// ```text
/// docker exec node bash -c 'command' >output 2>&1
/// ```
///
/// was exempt while still merging on the host, which is the defect the guard is named after. The
/// discriminator is whether the `2>&1` survives stripping the quoted spans.
fn merges_docker_exec_streams_on_the_host(logical: &str) -> bool {
    // `docker exec` must be the COMMAND, not a string handed to something else. Several suites
    // pass `--server-exec "docker exec -i $NODE bash"` to `run-suite.sh` and then redirect
    // run-suite.sh's own output — that is a host process merging its own two file descriptors
    // in-process, which is correct and unrelated. The discriminator is the character before: a
    // quote means the text is an argument.
    let at_command_position = logical
        .match_indices("docker exec")
        .any(|(at, _)| logical[..at].trim_end().chars().next_back().is_none_or(|c| !matches!(c, '"' | '\'')));
    if !at_command_position {
        return false;
    }
    let outside = outside_quotes(logical);
    // `/dev/null` is exempt: a readiness probe that discards its output has no order to lose.
    outside.contains("2>&1") && !outside.contains("/dev/null")
}

/// Positive and negative fixtures for the discriminator, rather than one substring exemption.
#[test]
fn the_docker_exec_stream_guard_separates_a_host_merge_from_an_in_container_one() {
    for bad in [
        // Plainest form: the redirect is the host shell's.
        r#"docker exec "$NODE" bin/ysqlsh -c 'SELECT 1' > "$OUT" 2>&1"#,
        // With `bash -c`, which the whole-line exemption used to let through. The command runs
        // in the container; the MERGE still happens on the host, after the closing quote.
        r#"docker exec node bash -c 'command' >output 2>&1"#,
        r#"docker exec "$N" bash -c "run" 2>&1 | tee "$LOG""#,
    ] {
        assert!(
            merges_docker_exec_streams_on_the_host(bad),
            "this merges on the host and must be rejected: {bad}"
        );
    }

    for good in [
        // The fix: the redirect is inside the single-quoted argument, so it is the CONTAINER's
        // shell that merges, at the source, where the order is real.
        r#"docker exec "$N" bash -c 'exec bin/ysqlsh "$@" 2>&1' x -c 'SELECT 1'"#,
        // Discarded, so there is no order to lose.
        r#"docker exec "$N" pg_isready >/dev/null 2>&1"#,
        // `docker exec` is an ARGUMENT here; the redirect belongs to the host process that is
        // merging its own descriptors, which is correct.
        r#"./run-suite.sh --server-exec "docker exec -i $NODE bash" > "$LOG" 2>&1"#,
    ] {
        assert!(
            !merges_docker_exec_streams_on_the_host(good),
            "this merges in the container, discards, or is not a docker exec command, and must \
             be accepted: {good}"
        );
    }
}

/// The EXECUTABLE body of a `just` recipe: its lines, with comment-only lines removed.
///
/// **The comment stripping is the point.** The guard above claims each suite is *run*, not merely
/// named — and the extraction it used to do copied the recipe's own indented comments into the
/// string it then searched. `gate-pg-release`'s comments are long and they name every script, by
/// design, because each paragraph explains which question that stage answers. So deleting
/// `run-yb-restore.sh` from the `SUITES` array while leaving the paragraph above it would have
/// kept the guard green: a false negative of exactly the shape the guard exists to prevent, in
/// the recipe that decides whether a revision may carry money.
fn executable_recipe_body(justfile: &str, name: &str) -> String {
    let mut body = String::new();
    let mut in_recipe = false;
    for line in justfile.lines() {
        if !in_recipe {
            // The header, e.g. `gate-pg-release jobs="1" tag="":`. The character after the name
            // must be a separator, so `check` does not match `gate-pg`.
            in_recipe = line.starts_with(name)
                && line[name.len()..].starts_with([' ', ':'])
                && line.trim_end().ends_with(':');
            continue;
        }
        // The next recipe or top-level item ends this one. An UNINDENTED comment does not end
        // it in the original scan, and does not here either — but it is not body, so it is
        // dropped by the rule below regardless.
        if !line.is_empty() && !line.starts_with([' ', '\t', '#']) {
            break;
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    body
}

/// The guard above must fail when the thing it guards is removed.
///
/// A guard is only worth its runtime if it goes red on the mutation it describes. This one was
/// documented as class-wide — "the suites must be RUN, not merely named in a comment" — while
/// being a plain substring search over text that INCLUDED those comments. It was never exercised
/// against a Justfile with the invocation deleted, so nothing had ever shown it could fail.
#[test]
fn the_release_composition_guard_fails_when_an_invocation_is_deleted() {
    // A recipe in the real shape: a paragraph naming the script, then the line that runs it.
    let genuine = "\
gate-pg-release jobs=\"1\":
    #!/usr/bin/env bash
    # Restore is in the RELEASE gate, not beside it -- run-yb-restore.sh proves a dump can be
    # replayed into a clean cluster, which is needed from the first production value.
    SUITES=(
        \"restore:./kamu-money-pg/yb/run-yb-restore.sh $NODE_IMAGE\"
    )

next-recipe:
    echo unrelated
";
    assert!(
        executable_recipe_body(genuine, "gate-pg-release").contains("run-yb-restore.sh"),
        "the extraction must still see an invocation that IS present"
    );

    // The mutation: the invocation deleted, the paragraph that explains it kept. This is what a
    // hurried "drop a suite for wall-clock" actually looks like in a diff.
    let mutated =
        genuine.replace("        \"restore:./kamu-money-pg/yb/run-yb-restore.sh $NODE_IMAGE\"\n", "");
    assert!(
        !executable_recipe_body(mutated.as_str(), "gate-pg-release").contains("run-yb-restore.sh"),
        "deleting the invocation while keeping the comment that names it must be VISIBLE to the \
         guard. It was not until 2026-07-26: the comments were part of the searched text, so the \
         guard passed on a gate that had stopped running the suite.\nbody was:\n{}",
        executable_recipe_body(mutated.as_str(), "gate-pg-release")
    );

    // And the name match must be EXACT, which needs a fixture where one name really is a prefix
    // of another or it proves nothing. The pair used to be `check` and `check-all`; after the
    // rename those became `gate-offline` and `gate-pg`, which share no prefix at all -- so the
    // guard would have gone on passing while exercising none of the behaviour it exists for.
    // `gate-pg` and `gate-pg-release` are the real prefix pair in this Justfile, and both names
    // exist, so a reader can check the fixture against the thing it models.
    let two = "gate-pg:\n    echo one\n\ngate-pg-release:\n    echo two\n";
    assert!(
        executable_recipe_body(two, "gate-pg").contains("echo one")
            && !executable_recipe_body(two, "gate-pg").contains("echo two"),
        "a prefix match would make every `gate-pg` assertion silently about `gate-pg-release` too"
    );
}

/// Does this logical line capture a gate's status by putting the gate's body in an `||` list?
///
/// bash does not apply `set -e` to "any command executed in a `&&` or `||` list except the command
/// following the final `&&` or `||`". So `{ body; } | tee "$LOG" || rc=$?` captures the status
/// correctly **and silently disables `set -e` for the whole body** — every stage runs regardless
/// of whether the one before it failed, and the group's exit status is whatever its LAST command
/// returned.
///
/// This is not theoretical. Measured on `gate-pg-release`, 2026-07-26: `just gate-pg` failed at
/// the workspace-lock self-test, the body carried on through the node-image build and all six
/// YugabyteDB suites, and the gate reported PASS — for a revision whose entire PostgreSQL matrix,
/// driver suites and text-adapter test had never run.
///
/// The correct shape is `set +e`, the pipeline as a plain command, `rc=${PIPESTATUS[0]}` read
/// immediately after it, `set -e`.
fn captures_status_by_disabling_set_e(logical: &str) -> bool {
    let Some(pipe) = logical.find("| tee") else {
        return false;
    };
    // `||` AFTER the pipe is the defect: the pipeline becomes the left operand of an `||` list.
    // A `||` before it belongs to some earlier command and says nothing about this one.
    logical[pipe..].contains("||")
}

#[test]
fn no_gate_captures_its_status_by_putting_its_body_in_an_or_list() {
    // The discriminator first, so a green result below means the scan works.
    for bad in [
        r#"} 2>&1 | tee "$LOG" || rc=$?"#,
        r#"} 2>&1 | tee "$LOG" || true"#,
        r#"{ stages; } | tee "$OUT" || rc=$?"#,
    ] {
        assert!(
            captures_status_by_disabling_set_e(bad),
            "this disables set -e for the body and must be rejected: {bad}"
        );
    }
    for good in [
        r#"} 2>&1 | tee "$LOG""#,
        r#"rc=${PIPESTATUS[0]}"#,
        // A `||` that is not downstream of the tee is unrelated.
        r#"command_that_may_fail || rc=$?"#,
        r#"[ -n "$X" ] || fail; echo done | tee "$LOG""#,
    ] {
        assert!(!captures_status_by_disabling_set_e(good), "this is fine and must be accepted: {good}");
    }

    let root = repo_root();
    let mut offenders = Vec::new();
    let mut files = vec!["Justfile".to_string()];
    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z", "*.sh"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs");
    for rel in listed.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        files.push(String::from_utf8_lossy(rel).into_owned());
    }

    for rel in files {
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            // Comments explain the defect at length; they are not the defect.
            if line.trim_start().starts_with('#') {
                continue;
            }
            if captures_status_by_disabling_set_e(line) {
                offenders.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "`set -e` does not apply inside an `||` list, so piping a gate's body through `tee` and \
         capturing the status with `|| rc=$?` lets every stage run after a failure and seals the \
         status of the LAST command. Use:\n  \
         set +e\n  {{ body; }} 2>&1 | tee \"$LOG\"\n  rc=${{PIPESTATUS[0]}}\n  set -e\n\n{}",
        offenders.join("\n")
    );
}

/// The boundary probe must not be able to reach a deployable artifact.
///
/// `boundary-probe` adds two no-op `#[pg_extern]`s to `kamu-money-pg` whose only purpose is to be
/// timed. They are useful, they are tracked so the measurement is reproducible, and they must
/// never ship: a money extension that exports `rs_noop` has a function in its SQL surface that
/// exists for a benchmark, and every consumer inherits it.
///
/// Three independent things keep it out, and each is a line someone can edit:
///   1. the cargo feature is not in `default`;
///   2. the YugabyteDB Dockerfile's `EXTRA_FEATURES` build-arg is empty by default;
///   3. the probe image is a separate `--target boundary-node`, never `node`.
///
/// `gate-pg-release` additionally greps the extracted artifact's own bytes. This test guards the
/// three sources so the failure is caught in `just check` rather than 25 minutes into a gate.
#[test]
fn the_boundary_probe_cannot_reach_a_deployable_artifact() {
    let root = repo_root();

    // 1. Not a default feature, and not pulled in by one.
    let manifest = std::fs::read_to_string(root.join("kamu-money-pg/Cargo.toml"))
        .expect("kamu-money-pg manifest is readable");
    let default_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("default ="))
        .expect("kamu-money-pg declares a default feature set");
    assert!(
        !default_line.contains("boundary-probe"),
        "`boundary-probe` is in the DEFAULT feature set, so an ordinary build ships two no-op \
         functions that exist only to be benchmarked: {default_line}"
    );
    assert!(
        manifest.contains("boundary-probe = []"),
        "`boundary-probe` must be a leaf feature that enables nothing else; if it grew \
         dependencies, re-read what they drag in before changing this assertion"
    );

    // 2. The Dockerfile build-arg is empty by default, so the release path is unchanged unless a
    //    caller names the feature.
    let dockerfile =
        std::fs::read_to_string(root.join("kamu-money-pg/yb/Dockerfile")).expect("yb Dockerfile is readable");
    let arg = dockerfile
        .lines()
        .find(|l| l.trim_start().starts_with("ARG EXTRA_FEATURES"))
        .expect("the yb Dockerfile declares EXTRA_FEATURES");
    assert_eq!(
        arg.trim(),
        "ARG EXTRA_FEATURES=",
        "EXTRA_FEATURES must default to EMPTY. A default value here silently changes what every \
         release build compiles, including the node image an orchestrator deploys: {arg}"
    );

    // 3. The probe image is its own target. `node` is what node-image.sh builds and what the gate
    //    certifies; it must not be the stage that copies the probe in.
    assert!(
        dockerfile.contains("FROM node AS boundary-node"),
        "the probe image must be a SEPARATE target built on top of `node`, so that building \
         `node` cannot produce an image carrying it"
    );
    let node_image_sh = std::fs::read_to_string(root.join("kamu-money-pg/yb/node-image.sh"))
        .expect("node-image.sh is readable");
    assert!(
        !node_image_sh.contains("boundary"),
        "node-image.sh builds the DEPLOYABLE image; it must not mention the probe at all"
    );

    // And the gate must check the shipped bytes, not just these three source facts.
    let justfile = std::fs::read_to_string(root.join("Justfile")).expect("Justfile is readable");
    let release = executable_recipe_body(&justfile, "gate-pg-release");
    assert!(
        release.contains("rs_noop"),
        "`gate-pg-release` must grep the EXTRACTED artifact for probe symbols. The three checks \
         above are properties of build files; this one is a property of the bytes that ship"
    );
}

/// A benchmark runner must print a host DIGEST, never the host's identity.
///
/// A transcript gets pasted into a design document, and a CPU model plus kernel string plus core count
/// fingerprints somebody's infrastructure. What a reader of two transcripts actually needs is
/// *"was this the same machine?"*, never *"which machine was it?"* — so the runners hash those
/// values and print twelve hex characters, and the raw strings survive only as hash inputs.
///
/// THE CONTENT SCAN LIVES IN `just scrub`, NOT HERE, and finding that out is why this test is
/// this small. `scrub` already greps the tracked tree for host paths, emails, private addresses,
/// credential prefixes, CPU models and the running user's own name — and it was in no gate, so
/// it had never once failed a build. A second, worse copy in Rust would have left two scanners
/// disagreeing about which is authoritative. This keeps only the half `scrub` cannot express: a
/// property of the runners' SHAPE rather than of any file's contents.
#[test]
fn a_bench_runner_prints_a_host_digest_and_never_the_host_itself() {
    let root = repo_root();
    for entry in std::fs::read_dir(root.join("kamu-money-pg/bench")).expect("bench dir") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if !name.starts_with("run-bench-") || !name.ends_with(".sh") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("runner is readable");
        assert!(
            text.contains("host id"),
            "{name} must print a non-identifying `host id` digest so two transcripts can be \
             compared without naming the machine"
        );
        for (i, line) in text.lines().enumerate() {
            let echoes = line.trim_start().starts_with("echo");
            let hashed = line.contains("sha256sum");
            assert!(
                !(echoes && line.contains("uname") && !hashed),
                "{name}:{}: prints a raw kernel string; feed it to the host-id digest instead",
                i + 1
            );
            assert!(
                !(echoes && line.contains("model name") && !hashed),
                "{name}:{}: prints a raw CPU model; feed it to the host-id digest instead",
                i + 1
            );
        }
    }
}

/// Strip `$( … )` spans, so a guard about ARGUMENT POSITIONS is not confused by substitutions.
fn without_command_substitutions(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes: Vec<char> = line.chars().collect();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '$' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            depth += 1;
            i += 2;
            continue;
        }
        if depth > 0 && bytes[i] == ')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 {
            out.push(bytes[i]);
        }
        i += 1;
    }
    out
}

/// Does this line let an EMPTY parameter shift the arguments after it?
///
/// `just` interpolates textually and the shell then word-splits, so a recipe parameter that
/// defaults to `""` VANISHES and every argument after it slides down one position.
///
/// Measured on `bench-why-yb`: `run-bench-sql-yb.sh {{ rows }} {{ passes }} {{ tag }} <fixture>`
/// with an empty `tag` handed the fixture path to the script as its THIRD argument — the image
/// tag — and `yb-image.sh` tried to `docker pull` a `.sql` file. Quoting preserves the empty
/// argument.
///
/// ONLY PARAMETERS THAT CAN BE EMPTY MATTER, which the first version of this guard missed: it
/// flagged `{{ rows }}`, whose default is `"100000"` and which therefore cannot collapse. A
/// guard that fires on the safe case is the false positive `just scrub`'s own header warns about.
fn shifts_arguments_when_a_parameter_is_empty(line: &str, empty_params: &[String]) -> bool {
    let l = without_command_substitutions(line);
    let trimmed = l.trim_start();
    // Only direct script invocations; `just foo {{ x }}` cannot shift a path into an argument slot.
    if !(trimmed.starts_with("./") || trimmed.contains(".sh ")) {
        return false;
    }
    for param in empty_params {
        let needle = format!("{{{{ {param} }}}}");
        let Some(at) = l.find(&needle) else {
            continue;
        };
        let quoted =
            l[..at].trim_end().ends_with('"') && l[at + needle.len()..].trim_start().starts_with('"');
        let follows = !l[at + needle.len()..].trim().is_empty();
        if !quoted && follows {
            return true;
        }
    }
    false
}

/// Recipe parameters whose default is the empty string, per recipe header.
fn recipes_with_empty_defaults(justfile: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for line in justfile.lines() {
        if line.starts_with([' ', '\t', '#']) || !line.trim_end().ends_with(':') {
            continue;
        }
        let header = line.trim_end().trim_end_matches(':');
        let mut parts = header.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let empty: Vec<String> = parts.filter_map(|p| p.strip_suffix("=\"\"").map(str::to_owned)).collect();
        if !empty.is_empty() {
            out.push((name.to_owned(), empty));
        }
    }
    out
}

#[test]
fn no_recipe_lets_an_empty_parameter_shift_the_arguments_after_it() {
    let tag = vec!["tag".to_string()];
    // Rejected: unquoted, empty-defaulting, and something follows.
    assert!(shifts_arguments_when_a_parameter_is_empty(
        "    ./x/run.sh {{ rows }} {{ passes }} {{ tag }} some/fixture.sql",
        &tag
    ));
    // Accepted: quoted, so the empty argument survives as an empty argument.
    assert!(!shifts_arguments_when_a_parameter_is_empty(
        "    ./x/run.sh {{ rows }} {{ passes }} \"{{ tag }}\" some/fixture.sql",
        &tag
    ));
    // Accepted: last position cannot shift anything.
    assert!(!shifts_arguments_when_a_parameter_is_empty(
        "    ./x/run.sh {{ rows }} {{ passes }} {{ tag }}",
        &tag
    ));
    // Accepted: inside a command substitution the argument belongs to the inner command, and
    // every script here defaults its own `${1:-…}`.
    assert!(!shifts_arguments_when_a_parameter_is_empty(
        "    ./x/run.sh \"$(./y/img.sh {{ tag }})\" \"\" {{ rows }}",
        &tag
    ));

    let root = repo_root();
    let justfile = std::fs::read_to_string(root.join("Justfile")).expect("Justfile is readable");
    let recipes = recipes_with_empty_defaults(&justfile);
    assert!(
        !recipes.is_empty(),
        "expected to find at least one recipe with an empty-defaulting parameter; if the parser          stopped matching headers this guard would pass by finding nothing"
    );

    let mut offenders = Vec::new();
    let mut current: Option<&Vec<String>> = None;
    for (i, line) in justfile.lines().enumerate() {
        if !line.starts_with([' ', '\t', '#']) && line.trim_end().ends_with(':') {
            let name = line.split_whitespace().next().unwrap_or("");
            current = recipes.iter().find(|(n, _)| n == name).map(|(_, p)| p);
            continue;
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(params) = current
            && shifts_arguments_when_a_parameter_is_empty(line, params)
        {
            offenders.push(format!("Justfile:{}: {}", i + 1, line.trim()));
        }
    }
    assert!(
        offenders.is_empty(),
        "an empty `just` parameter is word-split away and every argument after it slides down a \
         position — quote the interpolation:\n{}",
        offenders.join("\n")
    );
}

/// Every script that touches the fixed scratch paths takes the single-writer lock.
///
/// THE ENUMERATION IS DERIVED, NOT KEPT BY HAND. Anything tracked that names
/// `kamu-money-pg/yb/out` is a reader or a writer of state every other suite shares, so it is in
/// scope on the day it is added rather than the day someone remembers to list it. The only
/// exemptions are named below, and each one is itself checked.
///
/// WHY. A 2026-07-26 review found the lock covering `gate-pg-release` and almost nothing else:
/// `yb-build` took none at all, and `yb-ab`/`yb-native` wrote the shared artefact triplet and
/// THEN reached a locked runner, so the refusal arrived after the overwrite. Timed around
/// artifact extraction, that binds one node image beside another build's hashes — and pgrx's
/// generated SQL is not reproducible between builds, so the substitution is not merely a
/// theoretical byte difference.
///
/// `workspace-lock-selftest.sh` proves the runtime half: it holds the lock and starts every public
/// entry point. This is the static half, and it is the one that covers a script added tomorrow.
#[test]
fn every_script_that_touches_shared_scratch_takes_the_workspace_lock() {
    let root = repo_root();

    // SOURCED LIBRARIES, which run inside a caller that has already locked. Locking here would be
    // harmless but misleading: it would suggest these are entry points. Each is checked below to
    // be genuinely sourced, so this list cannot quietly become a way to opt out.
    let sourced_libraries = ["artifact.sh", "cluster.sh", "install.sh"];
    let read_only: [&str; 0] = [];

    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z", "*.sh"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs");

    let mut offenders = Vec::new();
    let mut seen_libraries = Vec::new();
    for rel in listed.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let rel = String::from_utf8_lossy(rel).into_owned();
        let base = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        if !text.contains("kamu-money-pg/yb/out") {
            continue;
        }
        if sourced_libraries.contains(&base.as_str()) {
            seen_libraries.push(base);
            continue;
        }
        if read_only.contains(&base.as_str()) {
            continue;
        }
        if !text.contains("workspace_lock") {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "these read or write the fixed scratch paths under kamu-money-pg/yb/out and take no \
         workspace lock, so a run started while another holds it can overwrite state the other \
         is mid-way through hashing:\n{}",
        offenders.join("\n")
    );

    // AN EXEMPTION THAT STOPPED BEING TRUE IS AN EXEMPTION THAT HIDES A DEFECT. A "sourced
    // library" that nothing sources is an entry point wearing a pass.
    let all: Vec<String> = listed
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    for lib in sourced_libraries {
        assert!(
            seen_libraries.iter().any(|s| s == lib),
            "{lib} is exempted as a sourced library but no longer touches the shared paths — \
             drop the exemption"
        );
        let sourced_by_something = all.iter().any(|rel| {
            std::fs::read_to_string(root.join(rel))
                .map(|t| t.contains(&format!("source {lib}")) || t.contains(&format!("/{lib}")))
                .unwrap_or(false)
                && !rel.ends_with(lib)
        });
        assert!(
            sourced_by_something,
            "{lib} is exempted from the workspace lock as a sourced library, and nothing sources \
             it — so it is an entry point with no lock"
        );
    }
}
