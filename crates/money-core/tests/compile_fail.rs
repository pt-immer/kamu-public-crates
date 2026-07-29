//! Compile-time contract tests.
//!
//! Each `tests/ui/*.rs` case is compiled by `trybuild` as a **separate downstream crate**
//! depending on `kamu-money-core`, so privacy and sealing match downstream behavior.
//!
//! Committed `.stderr` files ensure each case fails for the intended reason. After a rustc
//! upgrade, run `TRYBUILD=overwrite cargo test -p kamu-money-core --test compile_fail` and
//! review every diagnostic change.

/// Cases are split by required feature.
///
/// `tests/ui_serde/` holds the cases whose subject lives behind the `serde` feature — today
/// `tests/ui_serde/` holds cases whose subject requires `serde`; the default suite remains
/// feature-independent and cannot pass for an unrelated missing import.
#[test]
fn claims_the_type_system_is_supposed_to_enforce_are_still_enforced() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    #[cfg(feature = "serde")]
    t.compile_fail("tests/ui_serde/*.rs");
}
