//! FX rates and typed money conversion.

use crate::Money;
use crate::StaticCurrency;
use crate::domain_impl::{POW10_SCALE, in_domain};
use crate::error_impl::{AmountError, RateError};
use crate::rounding_impl::{Rounding, div_round_i256};
use core::marker::PhantomData;
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

/// The two-leg kernel. Rounds **once**, at the end — see [`Money::convert_via`] for why that
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

/// A directed FX rate: how many `Quote` units one `Base` unit buys.
///
/// The value uses the crate's fixed [`SCALE`](crate::advanced::domain::SCALE).
///
/// The pair is carried in the type, so `Money<USD>` can only be converted by a
/// `Rate<USD, IDR>` and the result can only be `Money<IDR>` — a mismatched pair does not
/// compile.
///
/// Rates are strictly positive and domain-bounded. Every constructor and decoding adapter
/// enforces those runtime invariants.
///
/// There is deliberately no `inverse()` and no `compose()`: real FX has bid and ask, so
/// inverting or composing mid-rates fabricates a price nobody can trade at. Every pair is
/// stored in both directions; multi-leg conversion is [`Money::convert_via`], which rounds
/// once.
pub struct Rate<Base: StaticCurrency, Quote: StaticCurrency> {
    units: i128,
    // The value is currency-agnostic, so the phantom pair carries both type parameters.
    _pair: PhantomData<(Base, Quote)>,
}

// Manual impls avoid unnecessary `Clone`/`Copy` bounds on the phantom parameters.
impl<Base: StaticCurrency, Quote: StaticCurrency> Clone for Rate<Base, Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Base: StaticCurrency, Quote: StaticCurrency> Copy for Rate<Base, Quote> {}
impl<Base: StaticCurrency, Quote: StaticCurrency> PartialEq for Rate<Base, Quote> {
    fn eq(&self, o: &Self) -> bool {
        self.units == o.units
    }
}
impl<Base: StaticCurrency, Quote: StaticCurrency> Eq for Rate<Base, Quote> {}
impl<Base: StaticCurrency, Quote: StaticCurrency> core::fmt::Debug for Rate<Base, Quote> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Rate({} units, {}->{})", self.units, Base::CODE.alpha3(), Quote::CODE.alpha3())
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Rate<Base, Quote> {
    /// Construct from canonical units, reporting **why** a value was refused.
    ///
    /// Every text, serde, PostgreSQL, and sqlx ingress reaches this invariant owner.
    ///
    /// # Errors
    /// [`RateError::Amount`] if the magnitude leaves the domain, and
    /// [`RateError::NonPositive`] if `units <= 0`. The domain is tested **first**, so
    /// `i128::MIN` is reported as the magnitude bug it is rather than as a sign bug, while an
    /// in-domain `-2` is reported as the sign bug it is.
    #[inline]
    pub const fn try_from_units(units: i128) -> Result<Self, RateError> {
        if !in_domain(units) {
            return Err(RateError::Amount(AmountError::out_of_domain(units)));
        }
        if units <= 0 {
            return Err(RateError::NonPositive { attempted_units: units });
        }
        Ok(Self { units, _pair: PhantomData })
    }

    /// The canonical units. Read-only: reconstructing requires a checked constructor.
    #[inline]
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }
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
mod tests {
    use crate::Money;
    use crate::Rate;
    use crate::domain_impl::{DOMAIN_MAX, POW10_SCALE};
    use crate::error_impl::{AmountError, RateError};
    use crate::iso::{EUR, IDR, Iso4217, USD};
    use crate::rounding_impl::Rounding;
    use ethnum::I256;

    /// `major` whole currency units, as a rate.
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

    #[test]
    fn a_zero_or_negative_rate_is_refused_at_construction() {
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(0),
            Err(RateError::NonPositive { attempted_units: 0 }),
            "a zero rate would send the money to zero, silently and with no residue"
        );
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(-2 * POW10_SCALE),
            Err(RateError::NonPositive { attempted_units: -2 * POW10_SCALE }),
            "a negative rate would flip the sign of the money passing through it"
        );

        assert!(Rate::<USD, IDR>::try_from_units(0).is_err());
        assert!(Rate::<USD, IDR>::try_from_units(-1).is_err());

        // The smallest representable positive rate remains valid.
        assert!(Rate::<USD, IDR>::try_from_units(1).is_ok(), "1e-18 is positive and in domain");
    }

    /// Magnitude and sign failures remain distinguishable. Domain is tested first, so
    /// `i128::MIN` reports its magnitude failure.
    #[test]
    fn the_two_rate_refusals_are_reported_separately() {
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(i128::MIN),
            Err(RateError::Amount(AmountError::out_of_domain(i128::MIN))),
            "out of domain AND negative: the magnitude is the useful fact"
        );
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(-DOMAIN_MAX),
            Err(RateError::NonPositive { attempted_units: -DOMAIN_MAX }),
            "in domain, so the sign is the only thing wrong with it"
        );
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

    /// The magnitude bound is `Money`'s, unchanged. Only the sign rule differs between the two
    /// types, and it differs because only one of them is a price.
    #[test]
    fn rate_construction_enforces_the_same_magnitude_bound_as_money() {
        assert!(Rate::<USD, IDR>::try_from_units(DOMAIN_MAX).is_ok());
        assert!(Rate::<USD, IDR>::try_from_units(DOMAIN_MAX + 1).is_err());
        assert!(Rate::<USD, IDR>::try_from_units(i128::MIN).is_err(), "i128::MIN must not sneak in");
        // The upper bound is shared with `Money`; the lower bound is not, and this is the pair
        // that says so.
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX).is_ok(), "money is signed");
        assert!(Rate::<USD, IDR>::try_from_units(-DOMAIN_MAX).is_err(), "a rate is not");
    }
}
