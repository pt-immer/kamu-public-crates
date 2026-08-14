//! Public browsing levels.
//!
//! The pre-facade compatibility paths this file used to exercise were removed in 0.2.0.
//! `tests/ui/removed_compat_paths.rs` now asserts they are gone, so the removal stays enforced
//! rather than merely done.

use core::num::NonZeroU32;
use kamu_money_core::SplitParts;
use kamu_money_core::advanced::{arithmetic, domain, stable_hash};
use kamu_money_core::errors::AmountError;
use kamu_money_core::iso::USD;
use kamu_money_core::{Money, StaticCurrency};

fn consume_parts(_: SplitParts<USD>) {}

#[test]
fn common_code_stays_at_the_root_and_details_are_grouped() {
    let whole = Money::<USD>::try_from_major(3).unwrap();
    consume_parts(whole.split(NonZeroU32::new(2).unwrap()));

    assert_eq!(arithmetic::add_units(1, 2), Some(3));
    assert!(domain::in_domain(domain::DOMAIN_MAX));
    assert_ne!(stable_hash::stable_hash(USD::CODE.numeric(), whole.units()), 0);
    assert_eq!(
        Money::<USD>::try_from_units(domain::DOMAIN_MAX + 1),
        Err(AmountError::out_of_domain(domain::DOMAIN_MAX + 1))
    );
}

#[test]
fn every_name_the_compatibility_paths_offered_has_a_home() {
    // The replacements, exercised as a set. Each of these was reachable from the crate root or a
    // module alias before 0.2.0, and the migration note on the removed item named this path.
    use kamu_money_core::advanced::residue::UntaggedDivision;
    use kamu_money_core::errors::{AllocationError, LocaleError, ParseMoneyError, RateError, WireError};

    let _: Option<UntaggedDivision> = None;
    let _: Option<(AllocationError, LocaleError, ParseMoneyError, RateError, WireError)> = None;
    let _: AmountError = AmountError::out_of_domain(domain::DOMAIN_MAX + 1);
    assert_eq!(domain::SCALE, 18);
    assert_eq!(<USD as StaticCurrency>::CODE, Money::<USD>::try_from_major(1).unwrap().code());
}
