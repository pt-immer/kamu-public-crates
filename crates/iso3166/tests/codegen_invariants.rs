//! Codegen regression guards.
//!
//! `build.rs` compiles the public tables from the vendored CSVs at the pinned
//! submodule commit. These pinned cardinalities catch accidental drift (a
//! submodule bump, a dedup/sort change in `build/csv_model.rs`, etc.). If the
//! vendored data is intentionally updated, bump the constants here together
//! with `VENDORED.md`.

#![allow(missing_docs)]
#![forbid(unsafe_code)]

use kamu_iso3166::Alpha2;
use kamu_iso3166::subdivision::SUBDIVISION_COUNT;

/// ISO 3166-1 countries at the pinned upstream commit
/// (`1224d32fecbec52b21dc5b18e327fa9c09cb1c92`).
const EXPECTED_COUNTRIES: usize = 249;

/// Distinct ISO 3166-2 subdivision codes after English-preferred dedup of the
/// 6260 upstream rows.
const EXPECTED_SUBDIVISIONS: usize = 5046;

#[test]
fn country_cardinality_is_pinned() {
    assert_eq!(
        Alpha2::COUNT,
        EXPECTED_COUNTRIES,
        "country count drifted from the pinned vendored data — review and update VENDORED.md",
    );
}

#[test]
fn subdivision_cardinality_is_pinned() {
    assert_eq!(
        SUBDIVISION_COUNT, EXPECTED_SUBDIVISIONS,
        "subdivision count drifted from the pinned vendored data — review and update VENDORED.md",
    );
}
