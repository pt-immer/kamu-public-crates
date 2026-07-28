use core::num::NonZeroU32;
use kamu_money_core::advanced::arithmetic::div_int_units;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::errors::AmountError;
use kamu_money_core::iso::USD;
use kamu_money_core::{Money, Residue, Rounding};

#[test]
fn residue_constructor_enforces_the_amount_domain() {
    let residue = Residue::<USD>::try_from_units(7).unwrap();
    assert_eq!(residue.units(), 7);
    assert_eq!(residue.code().alpha3(), "USD");
    assert_eq!(format!("{residue:?}"), "Residue { currency: \"USD\", units: 7 }");

    assert_eq!(
        Residue::<USD>::try_from_units(DOMAIN_MAX + 1).unwrap_err(),
        AmountError::out_of_domain(DOMAIN_MAX + 1)
    );
    assert_eq!(
        Residue::<USD>::try_from_units(-DOMAIN_MAX - 1).unwrap_err(),
        AmountError::out_of_domain(-DOMAIN_MAX - 1)
    );
}

#[test]
fn discard_deliberately_is_silent() {
    Residue::<USD>::try_from_units(7).unwrap().discard_deliberately();
}

#[test]
fn take_units_absorbs() {
    let r = Residue::<USD>::try_from_units(7).unwrap();
    assert_eq!(r.take_units(), 7);
}

#[test]
fn dropping_a_residue_never_panics() {
    drop(Residue::<USD>::try_from_units(1).unwrap());
}

/// `div_int` hands back ONE value, not a tuple, and there is no way to reach the quotient
/// without saying what happens to the residue.
///
/// The tuple was the defect: two values can be separated. One value cannot be,
/// so the obligation travels with the quotient until the caller chooses an exit.
#[test]
fn a_division_cannot_yield_its_quotient_without_a_decision() {
    let ten = || Money::<USD>::try_from_units(10_000_000_000_000_000_000).unwrap();
    let three = NonZeroU32::new(3).unwrap();

    // Take the residue and post it.
    let division = ten().div_int(three, Rounding::TowardZero);
    assert_eq!(division.residue_units(), 1);
    assert_eq!(
        format!("{division:?}"),
        "Division { currency: \"USD\", quotient: 3333333333333333333, residue: 1 }"
    );
    let (share, residue) = division.take_residue();
    assert_eq!(share.units(), 3_333_333_333_333_333_333);
    assert_eq!(residue.take_units(), 1, "the lost unit is handed back");

    // Or discard it explicitly.
    let share = ten().div_int(three, Rounding::TowardZero).discard_deliberately();
    assert_eq!(share.units(), 3_333_333_333_333_333_333);
}

#[test]
fn untagged_division_exposes_the_same_two_decisions_for_adapters() {
    let three = NonZeroU32::new(3).unwrap();

    let division = div_int_units(10, three, Rounding::TowardZero).unwrap();
    assert_eq!(division.residue_units(), 1);
    assert_eq!(format!("{division:?}"), "UntaggedDivision { quotient: 3, residue: 1 }");
    assert_eq!(division.take_residue(), (3, 1));

    let quotient = div_int_units(10, three, Rounding::TowardZero).unwrap().discard_deliberately();
    assert_eq!(quotient, 3);
}

/// Dropping an undecided `Division` is SAFE, and that is the whole point.
///
/// No panic, because no money was handed out — the quotient never escaped, so nothing left
/// the ledger. Compare the old tuple API, where the caller already held the quotient and the
/// runtime bomb was the only thing standing behind `let (share, _) = ...`.
#[test]
fn dropping_an_undecided_division_is_silent_because_nothing_escaped() {
    let m = Money::<USD>::try_from_units(10_000_000_000_000_000_000).unwrap();
    let _ = m.div_int(NonZeroU32::new(3).unwrap(), Rounding::TowardZero);
    // reaching here without a panic IS the assertion
}
