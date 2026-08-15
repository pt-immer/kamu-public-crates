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

/// The raw-unit and codec surfaces `kamu-money-pg` compiles against, by their published names.
///
/// Every other test in this crate lives under `src` and reaches these through `crate::`, so a
/// `pub` narrowed to `pub(crate)` would leave the whole in-crate suite green and break the
/// extension lane, which the root gate does not build.
#[test]
fn the_raw_unit_and_codec_surfaces_stay_public() {
    use kamu_money_core::advanced::residue::UntaggedDivision;
    use kamu_money_core::{Iso4217, Rounding, text};

    assert_eq!(arithmetic::sub_units(3, 1), Some(2));
    assert_eq!(arithmetic::sum_units([1, 2, 3]).unwrap(), 6);
    assert_eq!(arithmetic::UnitSum::ZERO.add_units(5).unwrap().finish().unwrap(), 5);
    assert_eq!(arithmetic::allocate_units(10, &[1, 1]).unwrap(), vec![5, 5]);

    let three = NonZeroU32::new(3).unwrap();
    let division: UntaggedDivision = arithmetic::div_int_units(10, three, Rounding::TowardZero).unwrap();
    assert_eq!(division.take_residue(), (3, 1));
    assert_eq!(Rounding::from_name("toward_zero"), Some(Rounding::TowardZero));

    let rendered = text::render(domain::POW10_SCALE, Iso4217::USD).unwrap();
    assert_eq!(rendered, "USD 1.00");
    assert_eq!(text::parse(&rendered).unwrap(), (Iso4217::USD, domain::POW10_SCALE));
    assert_eq!(text::parse_amount("1.00").unwrap(), domain::POW10_SCALE);
    assert_eq!(text::render_amount(Money::<USD>::try_from_major(1).unwrap()), "1.00");
}

/// `text::parse_amount` and `money!` are `const` as a published commitment.
///
/// A `const` item forces the compiler to evaluate them, and it does so from outside the crate,
/// which is where the commitment applies.
#[test]
fn the_const_parser_evaluates_for_a_downstream_crate() {
    use kamu_money_core::errors::ParseMoneyError;
    use kamu_money_core::money;

    const ONE_MAJOR: Result<i128, ParseMoneyError> = kamu_money_core::text::parse_amount("1.00");
    const RENT: Money<USD> = money!(USD, "1500.00");

    assert_eq!(ONE_MAJOR, Ok(domain::POW10_SCALE));
    assert_eq!(RENT.units(), 1_500 * domain::POW10_SCALE);
}

/// The locale builders, which are the crate's last surface before a number reaches a person.
#[test]
fn the_locale_policy_builders_stay_public() {
    use kamu_money_core::Iso4217;
    use kamu_money_core::locale::{EN_USD, FractionDigits, LocalePolicy, SymbolPosition};

    let policy = LocalePolicy::new(Iso4217::USD, "$")
        .with_symbol_position(SymbolPosition::Prefix)
        .try_with_separators(",", ".")
        .unwrap()
        .try_with_grouping(&[3])
        .unwrap()
        .with_min_fraction_digits(FractionDigits::try_new(2).unwrap());

    let m = Money::<USD>::try_from_major(1234).unwrap();
    assert_eq!(policy.render(m).unwrap(), "$1,234.00");
    assert_eq!(EN_USD.render(m).unwrap(), "$1,234.00");
    assert_eq!(EN_USD.render_units(m.units(), Iso4217::USD).unwrap(), "$1,234.00");
}

/// The per-field serde selectors, spelled exactly as a downstream `#[serde(with = ...)]` does.
#[cfg(feature = "serde")]
#[test]
fn the_wire_mode_paths_stay_public() {
    use kamu_money_core::iso::IDR;
    use kamu_money_core::{Money, Rate};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Payment {
        amount: Money<USD>,
        #[serde(with = "kamu_money_core::wire::structured")]
        tax: Money<USD>,
        #[serde(with = "kamu_money_core::wire::transparent")]
        fee: Money<USD>,
        #[serde(with = "kamu_money_core::wire::transparent")]
        rate: Rate<USD, IDR>,
    }

    let p = Payment {
        amount: Money::<USD>::try_from_major(10).unwrap(),
        tax: Money::<USD>::try_from_major(1).unwrap(),
        fee: Money::<USD>::try_from_units(1_500_000_000_000_000_000).unwrap(),
        rate: Rate::<USD, IDR>::try_from_units(16_000 * domain::POW10_SCALE).unwrap(),
    };
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(
        json,
        r#"{"amount":{"currency":"USD","amount":"10.00"},"tax":{"currency":"USD","amount":"1.00"},"fee":"USD 1.50","rate":"USD/IDR/16000"}"#
    );
    assert_eq!(serde_json::from_str::<Payment>(&json).unwrap(), p);
}
