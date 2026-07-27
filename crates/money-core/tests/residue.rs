use core::num::NonZeroU32;
use kamu_money_core::iso::USD;
use kamu_money_core::money::Money;
use kamu_money_core::residue::Residue;
use kamu_money_core::rounding::Rounding;

#[test]
fn zero_residue_drops_quietly() {
    let r = Residue::<USD>::new(0);
    drop(r); // nothing was lost, so there is nothing to absorb
}

#[test]
fn discard_deliberately_is_silent() {
    let r = Residue::<USD>::new(7);
    r.discard_deliberately(); // explicit, greppable, auditable
}

#[test]
fn take_units_absorbs() {
    let r = Residue::<USD>::new(7);
    assert_eq!(r.take_units(), 7);
}

/// An unabsorbed nonzero residue is a hard error in **every** profile.
///
/// Note what this test does NOT have: a `#[cfg(debug_assertions)]` split. An earlier design
/// panicked in debug and merely incremented a global counter in release, so this test needed
/// two arms — and its release arm asserted that the loss was *silent*, which is exactly the
/// behaviour a ledger cannot tolerate. Behaviour no longer depends on the build profile, so
/// neither does the test.
#[test]
fn dropping_an_unabsorbed_residue_panics_in_every_profile() {
    let caught = std::panic::catch_unwind(|| {
        let _r = Residue::<USD>::new(1);
    });
    let e = caught.expect_err("an unabsorbed residue must detonate, release included");
    let msg = e
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| e.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(msg.contains("unabsorbed Residue"), "wrong panic: {msg}");
}

/// `div_int` hands back ONE value, not a tuple, and there is no way to reach the quotient
/// without saying what happens to the residue.
///
/// This is the change that makes the drop-bomb a backstop instead of the only enforcement.
/// The tuple was the defect: two values can be separated, so the second one needed policing.
/// One value cannot be, so the obligation travels with the money. (specs.md C5)
#[test]
fn a_division_cannot_yield_its_quotient_without_a_decision() {
    let ten = || Money::<USD>::from_units(10_000_000_000_000_000_000).unwrap();
    let three = NonZeroU32::new(3).unwrap();

    // (a) take it: you asked for the obligation, so you now hold a Residue and its bomb.
    let (share, residue) = ten().div_int(three, Rounding::TowardZero).take_residue();
    assert_eq!(share.units(), 3_333_333_333_333_333_333);
    assert_eq!(residue.take_units(), 1, "the lost unit is handed back");

    // (b) throw it away on purpose: no Residue is ever constructed, so no bomb can fire.
    let share = ten().div_int(three, Rounding::TowardZero).discard_deliberately();
    assert_eq!(share.units(), 3_333_333_333_333_333_333);
}

/// Dropping an undecided `Division` is SAFE, and that is the whole point.
///
/// No panic, because no money was handed out — the quotient never escaped, so nothing left
/// the ledger. Compare the old tuple API, where the caller already held the quotient and the
/// runtime bomb was the only thing standing behind `let (share, _) = ...`.
#[test]
fn dropping_an_undecided_division_is_silent_because_nothing_escaped() {
    let m = Money::<USD>::from_units(10_000_000_000_000_000_000).unwrap();
    let _ = m.div_int(NonZeroU32::new(3).unwrap(), Rounding::TowardZero);
    // reaching here without a panic IS the assertion
}

// The wildcard-destructure case used to live here as a RUNTIME test: `let (share, _) = ...`
// warns about nothing, rustc actively suggests the `_` prefix that defeats `#[must_use]`, and
// the drop-bomb was the only thing standing behind it. It has moved to
// `tests/ui/residue_wildcard_destructure.rs`, because `div_int` no longer returns a tuple and
// the pattern is now a COMPILE error. A test that asserted a runtime panic would be asserting
// the weaker guarantee, and would go green again the moment someone reintroduced the tuple.

/// The drop-bomb must NOT fire during an unwind: a panic inside `Drop` while already panicking
/// aborts the process. This is the one hole in "hard error in every profile" — a residue
/// dropped during an unwind vanishes silently — and it is unavoidable in Rust. Aborting would
/// be strictly worse than the loss going unreported, and the operation that produced the
/// residue is already failing.
#[test]
fn drop_bomb_does_not_fire_during_unwind() {
    let caught = std::panic::catch_unwind(|| {
        let _r = Residue::<USD>::new(1);
        panic!("original failure");
    });
    let err = caught.expect_err("must unwind");
    let msg = err.downcast_ref::<&str>().copied().unwrap_or("");
    assert_eq!(msg, "original failure", "the ORIGINAL panic must survive, not be replaced by the bomb");
}
