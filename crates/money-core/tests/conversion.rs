//! FX conversion properties over the domain.
//!
//! Every test here iterates **all** rounding modes rather than sampling one, so a mode-specific
//! defect cannot hide behind a lucky seed.
//!
//! Successful-path properties constrain inputs so success is structural and then unwrap.
//! Overflow behavior has separate properties that assert `Err`.

use kamu_money_core::Money;
use kamu_money_core::Rate;
use kamu_money_core::Rounding;
use kamu_money_core::advanced::domain::{DOMAIN_MAX, POW10_SCALE, in_domain};
use kamu_money_core::errors::RateError;
use kamu_money_core::iso::{EUR, IDR, Iso4217, USD};
use proptest::prelude::*;

/// Widest operand bound that keeps a one-leg result in the domain by construction:
/// `1e26 * 1e26 / 1e18 = 1e34`, comfortably inside `DOMAIN_MAX` (`1e36 - 1`).
const IN_DOMAIN_OPERAND: i128 = 100_000_000_000_000_000_000_000_000;

/// Same, for two legs: `1e23 * 1e23 * 1e23 / 1e36 = 1e33`.
const IN_DOMAIN_OPERAND_VIA: i128 = 100_000_000_000_000_000_000_000;

fn usd(units: i128) -> Money<USD> {
    Money::<USD>::try_from_units(units).unwrap()
}

proptest! {
    /// Conversion never panics and never wraps, anywhere in the domain, under any mode.
    ///
    /// Full-domain sampling primarily exercises totality and overflow refusal. Constrained
    /// properties below cover successful arithmetic.
    ///
    /// Money spans both signs; rates start at one because they are strictly positive.
    #[test]
    fn prop_convert_never_panics_anywhere_in_the_domain(
        units in -DOMAIN_MAX..=DOMAIN_MAX,
        rate_units in 1..=DOMAIN_MAX,
    ) {
        let rate = Rate::<USD, IDR>::try_from_units(rate_units).unwrap();
        for mode in Rounding::ALL {
            match usd(units).convert(rate, *mode) {
                Ok(out) => prop_assert!(in_domain(out.units()), "{mode:?}"),
                Err(e) => prop_assert_eq!(
                    e,
                    RateError::ConversionOverflow {
                        from: Iso4217::USD,
                        to: Iso4217::IDR,
                    },
                    "{:?}", mode
                ),
            }
        }
    }

    /// An amount and rate large enough to leave the domain must be refused, not wrapped.
    ///
    /// The `Err` branch, asserted deliberately rather than reached by accident.
    #[test]
    fn prop_convert_refuses_results_that_leave_the_domain(
        units in (DOMAIN_MAX / 1_000)..=DOMAIN_MAX,
        rate_units in (DOMAIN_MAX / 1_000)..=DOMAIN_MAX,
    ) {
        let rate = Rate::<USD, IDR>::try_from_units(rate_units).unwrap();
        for mode in Rounding::ALL {
            prop_assert_eq!(
                usd(units).convert(rate, *mode),
                Err(RateError::ConversionOverflow {
                    from: Iso4217::USD,
                    to: Iso4217::IDR,
                }),
                "{:?}", mode
            );
        }
    }

    /// A rate of exactly 1.0 preserves the units and changes only the currency.
    ///
    /// This pins the scale handling on its own. An off-by-one-order divisor — dividing by
    /// `10^17` or `10^19` — still produces plausible money for most inputs and would survive
    /// an example test; this property rejects either error immediately.
    #[test]
    fn prop_a_unit_rate_moves_the_currency_and_nothing_else(
        units in -DOMAIN_MAX..=DOMAIN_MAX,
    ) {
        let one = Rate::<USD, IDR>::try_from_units(POW10_SCALE).unwrap();
        for mode in Rounding::ALL {
            let out = usd(units).convert(one, *mode).unwrap();
            prop_assert_eq!(out.units(), units, "{:?}", mode);
            prop_assert_eq!(out.code(), Iso4217::IDR);
        }
    }

    /// A whole-number rate is exact, and every mode must agree — checked against an
    /// independently computed expectation rather than against another code path.
    ///
    /// The oracle is plain `i128` multiplication, independent of the conversion path.
    #[test]
    fn prop_a_whole_number_rate_is_exact_under_every_mode(
        units in -IN_DOMAIN_OPERAND..=IN_DOMAIN_OPERAND,
        rate_major in 1i128..=1_000_000_000,
    ) {
        let rate = Rate::<USD, IDR>::try_from_units(
            rate_major.checked_mul(POW10_SCALE).unwrap()).unwrap();
        let expected = units.checked_mul(rate_major).unwrap();
        for mode in Rounding::ALL {
            let out = usd(units).convert(rate, *mode).unwrap();
            prop_assert_eq!(out.units(), expected, "{:?}", mode);
        }
    }

    /// The same, through a bridge: two whole-number rates leave nothing to round, so the
    /// two-leg result must equal the plain product under every mode.
    #[test]
    fn prop_whole_number_rates_are_exact_through_a_bridge(
        units in -IN_DOMAIN_OPERAND_VIA..=IN_DOMAIN_OPERAND_VIA,
        first_major in 1i128..=1_000,
        second_major in 1i128..=1_000,
    ) {
        let first = Rate::<USD, EUR>::try_from_units(
            first_major.checked_mul(POW10_SCALE).unwrap()).unwrap();
        let second = Rate::<EUR, IDR>::try_from_units(
            second_major.checked_mul(POW10_SCALE).unwrap()).unwrap();
        let expected = units
            .checked_mul(first_major).unwrap()
            .checked_mul(second_major).unwrap();
        for mode in Rounding::ALL {
            let out = usd(units).convert_via(first, second, *mode).unwrap();
            prop_assert_eq!(out.units(), expected, "{:?}", mode);
        }
    }

    /// When rounding **cannot** differ, `convert_via` and two sequential conversions must
    /// agree exactly.
    ///
    /// Whole-number rates make the intermediate exact, so there is no remainder for the
    /// materialised balance to lose. That isolates the claim: `convert_via` differs from
    /// sequential conversion ONLY by where it rounds, never by what it computes. The
    /// companion unit test shows the other side — where rounding does differ, sequential
    /// destroys the money outright.
    #[test]
    fn prop_convert_via_matches_sequential_when_the_intermediate_is_exact(
        major in -1_000_000_000i128..=1_000_000_000,
        first_major in 1i128..=1_000,
        second_major in 1i128..=1_000,
    ) {
        let money = usd(major.checked_mul(POW10_SCALE).unwrap());
        let first = Rate::<USD, EUR>::try_from_units(
            first_major.checked_mul(POW10_SCALE).unwrap()).unwrap();
        let second = Rate::<EUR, IDR>::try_from_units(
            second_major.checked_mul(POW10_SCALE).unwrap()).unwrap();

        for mode in Rounding::ALL {
            let sequential = money
                .convert(first, *mode)
                .and_then(|mid| mid.convert(second, *mode))
                .unwrap();
            let via = money.convert_via(first, second, *mode).unwrap();
            prop_assert_eq!(sequential, via, "{:?}", mode);
        }
    }
}
