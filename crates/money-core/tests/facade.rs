//! Public browsing levels and one-release compatibility paths.

use core::num::NonZeroU32;
use kamu_money_core::advanced::{arithmetic, domain, stable_hash};
use kamu_money_core::allocation::SplitParts;
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
#[allow(deprecated)]
fn old_paths_remain_as_compiler_guided_migration_shims() {
    use kamu_money_core::currency::StaticCurrency as OldStaticCurrency;
    use kamu_money_core::domain::SCALE as OLD_SCALE;
    use kamu_money_core::error::AmountError as OldAmountError;
    use kamu_money_core::money::Money as OldMoney;

    let value: OldMoney<USD> = OldMoney::try_from_major(1).unwrap();
    let _: OldAmountError = AmountError::out_of_domain(domain::DOMAIN_MAX + 1);
    let _: kamu_money_core::AmountError = AmountError::out_of_domain(domain::DOMAIN_MAX + 1);
    let _: Option<kamu_money_core::UntaggedDivision> = None;
    assert_eq!(value.code(), <USD as OldStaticCurrency>::CODE);
    assert_eq!(OLD_SCALE, domain::SCALE);
    assert_eq!(kamu_money_core::SCALE, domain::SCALE);
}
