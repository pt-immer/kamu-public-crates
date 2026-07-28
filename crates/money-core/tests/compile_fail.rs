//! Proofs that things which must NOT compile, do not compile.
//!
//! Every case here is a load-bearing claim of the design that no runtime test can make.
//! An untested compile error is a claim that silently stops being true — nothing fails when
//! someone adds the impl that dissolves it.
//!
//! Each `tests/ui/*.rs` case is compiled by `trybuild` as a **separate downstream crate**
//! depending on `kamu-money-core`, which is what makes the sealed-trait case meaningful: it is
//! rejected the same way a real third-party crate would be, not by crate-internal privacy.
//!
//! The committed `.stderr` beside each case is the actual guard. Without it, a case would
//! "pass" for any compile error at all, including a typo — the compile-fail version of a
//! test that asserts nothing. Re-blessing after a rustc upgrade: `TRYBUILD=overwrite cargo
//! test -p kamu-money-core --test compile_fail`, then READ the diff before committing it. If a
//! re-blessed message no longer names the invariant the case exists to prove, that is a
//! finding, not a formatting change.

/// Cases are split across two directories BY THE FEATURE THEY NEED, not by subject.
///
/// `tests/ui_serde/` holds the cases whose subject lives behind the `serde` feature — today
/// that is `counterfeit_scalar.rs`, which pins the seal on `wire::transparent::Scalar`. Under
/// default features `wire` does not exist, so that case still fails to compile, but on an
/// unresolved import rather than the sealed-trait error it exists to prove. A compile-fail test
/// that fails for the wrong reason is exactly as worthless as one that passes for the wrong
/// reason, and the committed `.stderr` is what catches it.
///
/// The alternative was `required-features = ["serde"]` on this whole test target, which is
/// three lines shorter and quietly skips all six cases whenever the feature is off. A directory
/// split costs nothing at runtime, keeps the five feature-independent cases running everywhere,
/// and gives the next serde-dependent case an obvious home.
#[test]
fn claims_the_type_system_is_supposed_to_enforce_are_still_enforced() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    #[cfg(feature = "serde")]
    t.compile_fail("tests/ui_serde/*.rs");
}
