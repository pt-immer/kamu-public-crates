//! The fan-out this repository intends, written independently of the map that implements it.
//!
//! Deriving the expectation from `DERIVED_CLASSES` would move both sides of the assertion
//! together and prove nothing.

use std::collections::{BTreeMap, BTreeSet};

use repo_policy::ci_paths::{BASE_CLASSES, DERIVED_CLASSES, classify_path, classify_paths};

/// One path selecting exactly one base class, for every base class. Purity is asserted rather
/// than assumed, because an impure entry would let a derived class appear load-bearing on a
/// source it does not actually read.
fn representative_paths() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("docs", "README.md"),
        ("iso3166", "crates/iso3166/src/lib.rs"),
        ("logging", "crates/logging/src/lib.rs"),
        ("money", "crates/money-core/tests/facade.rs"),
        ("moneypg", "extensions/money-pg/Cargo.toml"),
        ("shared", "Cargo.lock"),
        ("shell", "ops/new-check.sh"),
        ("snap", "crates/snap-crypto/src/lib.rs"),
        ("tools", "tools/repo-policy/src/lib.rs"),
    ])
}

fn expected_fan_out() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    BTreeMap::from([
        ("rust", BTreeSet::from(["iso3166", "logging", "money", "snap", "shared", "tools"])),
        ("iso", BTreeSet::from(["iso3166", "shared"])),
        ("log", BTreeSet::from(["logging", "shared"])),
        ("money", BTreeSet::from(["money", "shared"])),
        ("snap", BTreeSet::from(["snap", "shared"])),
        ("moneypg", BTreeSet::from(["moneypg", "shared"])),
        ("worker", BTreeSet::from(["logging", "shared"])),
        ("lint", BTreeSet::from(BASE_CLASSES)),
        ("shell", BTreeSet::from(["shell"])),
    ])
}

#[test]
fn every_edge_names_a_base_class_and_a_reason() {
    let base: BTreeSet<&str> = BTreeSet::from(BASE_CLASSES);
    for derived in &DERIVED_CLASSES {
        assert!(!derived.sources.is_empty(), "{}: a class with no source can never fire", derived.name);
        assert!(
            !derived.reason.trim().is_empty(),
            "{}: state why working on these runs this class's jobs",
            derived.name
        );
        for source in derived.sources {
            assert!(base.contains(source), "{}: {source} is not a base class", derived.name);
        }
    }
}

#[test]
fn every_base_class_has_a_representative_path() {
    assert_eq!(BTreeSet::from(BASE_CLASSES), representative_paths().keys().copied().collect());
}

#[test]
fn each_representative_selects_exactly_its_own_base_class() {
    for (name, path) in representative_paths() {
        assert_eq!(BTreeSet::from([name]), classify_path(path), "{path} is not a pure {name}");
    }
}

#[test]
fn the_map_declares_exactly_the_intended_classes() {
    let declared: BTreeSet<&str> = DERIVED_CLASSES.iter().map(|derived| derived.name).collect();
    assert_eq!(expected_fan_out().keys().copied().collect::<BTreeSet<_>>(), declared);
}

#[test]
fn a_derived_class_fires_on_its_sources_and_on_nothing_else() {
    for (derived, expected) in expected_fan_out() {
        for (base, path) in representative_paths() {
            let classes = classify_paths([path]).expect("every representative is owned");
            assert_eq!(expected.contains(base), classes[derived], "{derived} on a {base} change ({path})");
        }
    }
}

#[test]
fn every_tracked_path_has_an_owner() {
    let tracked = repo_policy::tracked(&["."]);
    classify_paths(&tracked).expect("every tracked path is classified");
}
