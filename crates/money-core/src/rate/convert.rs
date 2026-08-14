//! Applying a rate: one leg, or two with a single rounding at the end.

use crate::Money;
use crate::StaticCurrency;
use crate::domain::POW10_SCALE;
use crate::errors::RateError;
use crate::rate::Rate;
use crate::rounding::{Rounding, div_round_i256};
use ethnum::I256;

/// `POW10_SCALE^2`: the divisor for a two-leg conversion, which applies the scale twice.
///
/// Derived from [`POW10_SCALE`] so scale changes remain coupled. The product fits `i128`, and
/// const evaluation checks that bound at build time.
const POW10_SCALE_SQUARED: i128 = POW10_SCALE * POW10_SCALE;

/// The one-leg conversion kernel, shared by the typed and the runtime path.
///
/// Returns `None` iff the result does not fit `i128`; `Money` construction owns the domain
/// check.
fn apply_rate(units: i128, rate_units: i128, mode: Rounding) -> Option<i128> {
    // In-domain operands bound the product at 1e72, below I256::MAX.
    let product = I256::from(units)
        .checked_mul(I256::from(rate_units))
        .expect("|units| <= DOMAIN_MAX ~1e36 twice over, so |product| <= 1e72 < I256::MAX");

    let (quotient, _below_one_unit) = div_round_i256(product, I256::from(POW10_SCALE), mode);

    // The quotient can exceed i128; narrowing must stay checked.
    i128::try_from(quotient).ok()
}

/// The two-leg kernel. Rounds **once**, at the end — see [`Money::convert_via`](crate::Money::convert_via) for why that
/// is a ledger requirement rather than a precision one.
fn apply_rate_pair(units: i128, first: i128, second: i128, mode: Rounding) -> Option<i128> {
    // First leg: |m * r1| <= 1e72, the same proof as `apply_rate`. Cannot fail.
    let partial = I256::from(units)
        .checked_mul(I256::from(first))
        .expect("|units| <= DOMAIN_MAX ~1e36 twice over, so |product| <= 1e72 < I256::MAX");

    // Second leg: this one CAN overflow (1e72 * 1e36 = 1e108), and when it does the result
    // would have left the domain regardless — so refusing is correct, not conservative.
    let product = partial.checked_mul(I256::from(second))?;

    let (quotient, _below_one_unit) = div_round_i256(product, I256::from(POW10_SCALE_SQUARED), mode);

    i128::try_from(quotient).ok()
}

impl<C: StaticCurrency> Money<C> {
    /// Convert at `rate`, rounding per `mode`.
    ///
    /// The pair is checked by the type system: this value's currency must be the rate's
    /// **base**, and the result is denominated in the rate's **quote**. A mismatched pair does
    /// not compile.
    ///
    /// No [`Residue`](crate::Residue) is returned: conversion loss is strictly below one
    /// canonical unit and therefore cannot be represented as money.
    ///
    /// There is deliberately **no `impl Mul`**: an operator that fails on ordinary input is a
    /// lie, and this one does — `USD -> ZWL` at the 2008 rate leaves the domain at a $100 000
    /// balance.
    ///
    /// # Errors
    /// [`RateError::ConversionOverflow`] if the converted amount leaves the domain. That is a
    /// *condition*, not a bug: it is reachable at ordinary balances for high-magnitude pairs.
    ///
    /// # Panics
    /// Never. The `expect` below is proven unreachable by the domain invariant, and its proof
    /// is written at the site.
    #[must_use = "the converted money is the result; dropping it discards the conversion"]
    pub fn convert<Quote: StaticCurrency>(
        self,
        rate: Rate<C, Quote>,
        mode: Rounding,
    ) -> Result<Money<Quote>, RateError> {
        let units = apply_rate(self.units(), rate.units(), mode)
            .ok_or(RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })?;
        Money::<Quote>::try_from_units(units)
            .map_err(|_| RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })
    }

    /// Convert through a bridge currency, rounding **once**, at the end.
    ///
    /// This is a ledger rule: sequential conversions materialize and quantize a
    /// `Money<Bridge>` balance the holder never held. `convert_via` does not create that balance.
    ///
    /// This is also what callers reaching for a `compose()` actually want. Composing two
    /// mid-rates would fabricate a third that cannot be traded at, and its error grows
    /// *linearly with the amount*; the intermediate quantisation avoided here is absolute.
    ///
    /// # Errors
    /// [`RateError::ConversionOverflow`] if the conversion leaves the domain — including when
    /// the three-way product exceeds `I256`. An in-domain result implies
    /// `m*r1*r2 <= 1e72 < I256::MAX`, so this does not reject a representable result.
    ///
    /// # Panics
    /// Never. The `expect` below is proven unreachable by the domain invariant.
    #[must_use = "the converted money is the result; dropping it discards the conversion"]
    pub fn convert_via<Bridge: StaticCurrency, Quote: StaticCurrency>(
        self,
        first: Rate<C, Bridge>,
        second: Rate<Bridge, Quote>,
        mode: Rounding,
    ) -> Result<Money<Quote>, RateError> {
        let units = apply_rate_pair(self.units(), first.units(), second.units(), mode)
            .ok_or(RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })?;
        Money::<Quote>::try_from_units(units)
            .map_err(|_| RateError::ConversionOverflow { from: C::CODE, to: Quote::CODE })
    }
}

#[cfg(test)]
// The small-end cases state a relative resolution a human reads, which is what `f64` is for
// here; every value it touches has already been asserted exactly in canonical units above.
#[allow(clippy::as_conversions, clippy::cast_precision_loss)]
mod tests {
    use crate::domain::{DOMAIN_MAX, POW10_SCALE, in_domain};
    use crate::errors::RateError;
    use crate::iso::{EUR, IDR, Iso4217, USD};
    use crate::{Money, Rate, Rounding};
    use ethnum::I256;
    use proptest::prelude::*;

    fn rate<Base: crate::StaticCurrency, Quote: crate::StaticCurrency>(major: i128) -> Rate<Base, Quote> {
        Rate::try_from_units(major.checked_mul(POW10_SCALE).unwrap()).unwrap()
    }

    #[test]
    fn converting_yields_the_target_currency_at_the_quoted_price() {
        // $10.00 at 16 000 IDR/USD is Rp160 000.00 — exactly, no rounding involved.
        let usd = Money::<USD>::try_from_major(10).unwrap();
        let got = usd.convert(rate::<USD, IDR>(16_000), Rounding::HalfEven).unwrap();
        assert_eq!(got, Money::<IDR>::try_from_major(160_000).unwrap());
    }

    /// `convert_via` rounds once, at the end.
    ///
    /// USD -> EUR at 0.5, then EUR -> IDR at 2.0, applied to one canonical unit. The
    /// intermediate is half a unit, which the ledger cannot express, so a sequential
    /// conversion quantises it to zero and the second leg multiplies nothing by two. Via,
    /// `0.5 * 2 == 1` exactly and the unit survives — because no `Money<EUR>` balance the
    /// holder never held is ever created.
    ///
    #[test]
    fn convert_via_rounds_once_where_two_conversions_would_destroy_the_money() {
        let m = Money::<USD>::try_from_units(1).unwrap();
        let usd_eur = Rate::<USD, EUR>::try_from_units(POW10_SCALE / 2).unwrap();
        let eur_idr = Rate::<EUR, IDR>::try_from_units(2 * POW10_SCALE).unwrap();

        let sequential =
            m.convert(usd_eur, Rounding::HalfEven).unwrap().convert(eur_idr, Rounding::HalfEven).unwrap();
        assert_eq!(sequential.units(), 0, "the materialised intermediate ate the money");

        let via = m.convert_via(usd_eur, eur_idr, Rounding::HalfEven).unwrap();
        assert_eq!(via.units(), 1, "one rounding, at the end, and it survives");
    }

    /// `convert_via`'s second `checked_mul` is the one that can genuinely overflow, and
    /// rejecting is exact rather than conservative: an in-domain result implies
    /// `m*r1*r2 <= 1e72 < I256::MAX`, so anything that overflows would have left the domain
    /// anyway.
    #[test]
    fn convert_via_refuses_a_product_that_cannot_fit_the_intermediate() {
        let m = Money::<USD>::try_from_units(DOMAIN_MAX).unwrap();
        let huge_a = Rate::<USD, EUR>::try_from_units(DOMAIN_MAX).unwrap();
        let huge_b = Rate::<EUR, IDR>::try_from_units(DOMAIN_MAX).unwrap();
        assert_eq!(
            m.convert_via(huge_a, huge_b, Rounding::HalfEven),
            Err(RateError::ConversionOverflow { from: Iso4217::USD, to: Iso4217::IDR }),
            "1e108 does not fit I256, and the result would not have fit the domain either"
        );
    }

    /// Domain overflow in conversion is a runtime condition, so `convert` returns `Result` and
    /// there is no `impl Mul`.
    ///
    /// Both gates must report the same thing, and that is the point of this test: the
    /// quotient can be too big for `DOMAIN_MAX` while still fitting `i128`, or too big for
    /// `i128` outright. A caller cannot tell those apart and should not have to.
    #[test]
    fn conversion_overflow_names_the_pair_from_both_gates() {
        let m = Money::<USD>::try_from_units(DOMAIN_MAX).unwrap();
        let expected = RateError::ConversionOverflow { from: Iso4217::USD, to: Iso4217::IDR };

        // q = 1e37: outside DOMAIN_MAX (1e36), still inside i128 (~1.7e38).
        assert_eq!(
            m.convert(rate::<USD, IDR>(10), Rounding::HalfEven),
            Err(expected),
            "in i128, outside the domain"
        );
        // q = 1e39: outside i128 entirely, so `i128::try_from` is what refuses.
        assert_eq!(
            m.convert(rate::<USD, IDR>(1_000), Rounding::HalfEven),
            Err(expected),
            "outside i128 — this is why the error cannot name the attempted value"
        );
    }

    /// A quotient of exactly `2^128` would truncate to zero under an unchecked narrowing.
    /// This case therefore distinguishes checked narrowing from the later domain check.
    #[test]
    fn a_quotient_that_would_truncate_back_into_the_domain_is_still_refused() {
        // 2^64 * 10^9, comfortably in domain, chosen so the product is 2^128 * 10^18.
        let units = 18_446_744_073_709_551_616_000_000_000;
        let m = Money::<USD>::try_from_units(units).unwrap();
        let r = Rate::<USD, IDR>::try_from_units(units).unwrap();

        assert_eq!(
            m.convert(r, Rounding::HalfEven),
            Err(RateError::ConversionOverflow { from: Iso4217::USD, to: Iso4217::IDR }),
            "truncation would have made this a silent, perfectly plausible ZERO"
        );
    }

    proptest::proptest! {
        /// Conversion rounding must discard less than one canonical unit. Rates start at one
        /// because zero and negative values are outside the type's domain.
        #[test]
        fn prop_the_discarded_remainder_is_always_below_one_canonical_unit(
            units in -100_000_000_000_000_000_000_000_000i128..=100_000_000_000_000_000_000_000_000,
            rate_units in 1i128..=100_000_000_000_000_000_000_000_000,
        ) {
            let rate = Rate::<USD, IDR>::try_from_units(rate_units).unwrap();
            for mode in Rounding::ALL {
                let out = Money::<USD>::try_from_units(units).unwrap().convert(rate, *mode).unwrap();

                // exact: what the conversion was asked for, minus what it returned
                let product = I256::from(units)
                    .checked_mul(I256::from(rate_units))
                    .unwrap();
                let returned = I256::from(out.units())
                    .checked_mul(I256::from(POW10_SCALE))
                    .unwrap();
                let remainder = product.checked_sub(returned).unwrap();

                // `-one_unit` would be unary negation on I256, which trips
                // clippy::arithmetic_side_effects; negating the i128 const instead is a
                // const-folded expression and is exempt, same as POW10_SCALE_SQUARED above.
                let one_unit = I256::from(POW10_SCALE);
                let minus_one_unit = I256::from(-POW10_SCALE);
                proptest::prop_assert!(
                    remainder < one_unit && remainder > minus_one_unit,
                    "{mode:?}: rounding moved {remainder}, which is a whole canonical unit or \
                     more — conversion would need to return a Residue"
                );
            }
        }
    }

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

    /// Significant decimal digits available to a rate of a given magnitude.
    fn significant_digits(units: i128) -> u32 {
        units.unsigned_abs().checked_ilog10().map_or(0, |d| d.saturating_add(1))
    }

    /// Significant-digit headroom per decade.
    #[test]
    fn the_significant_digits_available_at_each_magnitude() {
        // (rate magnitude as a power of ten, units behind it, significant digits)
        let rows: &[(i32, i128, u32)] = &[
            (0, POW10_SCALE, 19), // 1.0
            (-3, 1_000_000_000_000_000, 16),
            (-6, 1_000_000_000_000, 13),
            (-9, 1_000_000_000, 10),
            (-13, 100_000, 6), // the case the contract named
            (-15, 1_000, 4),
            (-17, 10, 2),
            (-18, 1, 1), // one unit: the floor
        ];
        for &(exponent, units, expected) in rows {
            assert_eq!(significant_digits(units), expected, "rate 1e{exponent} has {units} units");
            assert!(Rate::<USD, IDR>::try_from_units(units).is_ok(), "1e{exponent} must be representable");
        }
    }

    /// **The contract's number was right.** A rate of `1e-13` holds six digits, so the seventh is
    /// not merely imprecise — it does not exist, and `try_from_units` cannot round to it.
    #[test]
    fn a_rate_near_1e_minus_13_carries_six_significant_digits() {
        let rate = 100_000i128; // 1e-13 at scale 18
        assert_eq!(significant_digits(rate), 6);

        // The next representable value up is one part in 100_000 away: ~1e-5 relative resolution.
        let next = rate + 1;
        assert_eq!(significant_digits(next), 6);
        let relative_step = 1.0f64 / (rate as f64);
        assert!(
            (relative_step - 1e-5).abs() < 1e-9,
            "resolution at 1e-13 is {relative_step}, expected ~1e-5"
        );
    }

    /// The floor: one unit is a usable rate, and it converts without collapsing to zero — but the
    /// money must be large enough to survive it. This is the honest limit.
    #[test]
    fn the_smallest_representable_rate_still_converts() {
        let smallest = Rate::<USD, IDR>::try_from_units(1).expect("1e-18 is in domain");

        // A big enough amount survives: 1e18 units * 1e-18 = 1 unit.
        let big = Money::<USD>::try_from_units(POW10_SCALE).expect("in domain");
        let out = big.convert(smallest, Rounding::TowardZero).expect("stays in domain");
        assert_eq!(out.units(), 1, "1.0 USD at 1e-18 is one IDR unit");

        // Anything smaller rounds to nothing — named, not hidden.
        let small = Money::<USD>::try_from_units(POW10_SCALE - 1).expect("in domain");
        let gone = small.convert(smallest, Rounding::TowardZero).expect("stays in domain");
        assert_eq!(
            gone.units(),
            0,
            "below 1.0 USD, a 1e-18 rate rounds to zero — the rate has no digits left to carry it"
        );
    }

    /// A realistic hyperinflation counter-direction, end to end. The forward rate is huge and the
    /// reverse is tiny, and both must be storable because there is no `inverse()`.
    #[test]
    fn both_directions_of_a_hyperinflation_pair_are_representable() {
        // Forward: 1 USD = 3,000,000 IDR-like units.
        let forward = Rate::<USD, IDR>::try_from_units(3_000_000 * POW10_SCALE).expect("in domain");
        // Reverse: 1/3e6 = 3.3333...e-7, which at scale 18 is 333_333_333_333 units.
        let reverse = Rate::<IDR, USD>::try_from_units(333_333_333_333).expect("in domain");

        assert_eq!(significant_digits(reverse.units()), 12, "12 digits survive");

        // A round trip loses at most the truncation, and it is bounded rather than silent.
        let start = Money::<USD>::try_from_major(1).expect("in domain");
        let there = start.convert(forward, Rounding::TowardZero).expect("in domain");
        let back = there.convert(reverse, Rounding::TowardZero).expect("in domain");

        let drift = (start.units() - back.units()).abs();
        assert!(
            drift < POW10_SCALE / 1_000_000,
            "round-trip drift {drift} units must stay below 1e-6 of a major unit"
        );
    }
}
