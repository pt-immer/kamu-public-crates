//! Every route into a `Money`, and the domain each one enforces.

use super::Money;
use crate::StaticCurrency;
use crate::domain::in_domain;
use crate::errors::AmountError;
use core::marker::PhantomData;

impl<C: StaticCurrency> Money<C> {
    /// Zero.
    pub const ZERO: Self = Self { units: 0, _c: PhantomData };

    /// Construct from canonical units.
    ///
    /// # Errors
    ///
    /// Returns [`AmountError`] when `units` lies outside the fixed domain.
    #[inline]
    pub const fn try_from_units(units: i128) -> Result<Self, AmountError> {
        if in_domain(units) {
            Ok(Self { units, _c: PhantomData })
        } else {
            Err(AmountError::out_of_domain(units))
        }
    }

    #[inline]
    pub(crate) const fn from_units_unchecked(units: i128) -> Self {
        Self { units, _c: PhantomData }
    }

    /// Construct from whole currency units (for example, `10` means
    /// `10.000000000000000000`).
    ///
    /// Takes `i128` so callers never narrow before entering the checked domain.
    ///
    /// # Errors
    ///
    /// Returns [`AmountError::OutOfDomain`] when the scaled value fits `i128` but leaves the
    /// money domain, or [`AmountError::MajorScaleOverflow`] when scaling itself overflows.
    #[inline]
    pub const fn try_from_major(major: i128) -> Result<Self, AmountError> {
        match major.checked_mul(crate::domain::POW10_SCALE) {
            Some(units) => Self::try_from_units(units),
            None => Err(AmountError::MajorScaleOverflow { attempted_major: major }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AmountError;
    use crate::iso::USD;
    use proptest::prelude::*;

    #[test]
    fn construction_enforces_the_domain() {
        assert!(Money::<USD>::try_from_units(0).is_ok());
        assert!(Money::<USD>::try_from_units(DOMAIN_MAX).is_ok());
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX).is_ok());
        assert!(Money::<USD>::try_from_units(DOMAIN_MAX + 1).is_err());
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX - 1).is_err());
        assert!(Money::<USD>::try_from_units(i128::MIN).is_err(), "i128::MIN must not sneak in");
    }
    /// Different construction routes produce the same canonical units.
    #[test]
    fn the_same_amount_reached_by_different_routes_is_the_same_value() {
        let direct = Money::<USD>::try_from_units(10_500_000_000_000_000_000).unwrap();
        let built = Money::<USD>::try_from_major(10).unwrap()
            + Money::<USD>::try_from_units(500_000_000_000_000_000).unwrap();
        assert_eq!(direct, built);
        assert_eq!(direct.units(), built.units());
    }
    #[test]
    fn from_major_scales_by_pow10() {
        assert_eq!(Money::<USD>::try_from_major(10).unwrap().units(), 10 * crate::domain::POW10_SCALE);
        assert_eq!(Money::<USD>::try_from_major(-3).unwrap().units(), -3 * crate::domain::POW10_SCALE);
        assert_eq!(Money::<USD>::ZERO.units(), 0);
    }
    /// The constructor spans the domain and distinguishes domain rejection from scaling
    /// overflow.
    #[test]
    fn from_major_spans_the_domain_and_rejects_beyond_it() {
        // Derived from the domain constants so the test follows a scale change.
        const MAX_MAJOR: i128 = DOMAIN_MAX / crate::domain::POW10_SCALE; // 10^18 - 1 at SCALE 18

        assert!(Money::<USD>::try_from_major(MAX_MAJOR).is_ok(), "top of the domain");
        assert!(Money::<USD>::try_from_major(-MAX_MAJOR).is_ok(), "bottom of the domain");
        assert!(
            Money::<USD>::try_from_major(MAX_MAJOR + 1).is_err(),
            "one major unit above the domain must be refused"
        );
        assert!(
            Money::<USD>::try_from_major(-MAX_MAJOR - 1).is_err(),
            "one major unit below the domain must be refused"
        );
        assert_eq!(
            Money::<USD>::try_from_major(i128::MAX),
            Err(AmountError::MajorScaleOverflow { attempted_major: i128::MAX })
        );
        assert_eq!(
            Money::<USD>::try_from_major(i128::MIN),
            Err(AmountError::MajorScaleOverflow { attempted_major: i128::MIN })
        );

        assert_eq!(
            MAX_MAJOR,
            10i128.pow(crate::domain::PRECISION - crate::domain::SCALE) - 1,
            "major range is 10^(PRECISION-SCALE) - 1, whatever SCALE happens to be"
        );
        assert!(Money::<USD>::try_from_major(0).is_ok());
    }

    proptest::proptest! {
    /// Weighted bands exercise accepted and rejected values on every run. Separate example tests
    /// pin exact boundaries that random sampling is unlikely to hit.
    #[test]
    fn prop_constructor_accepts_exactly_the_domain(
        u in prop_oneof![
            2 => -DOMAIN_MAX..=DOMAIN_MAX,
            1 => (DOMAIN_MAX + 1)..=i128::MAX,
            1 => i128::MIN..=(-DOMAIN_MAX - 1),
        ],
    ) {
        // Use an independent literal range rather than the implementation predicate.
        let inside = (-DOMAIN_MAX..=DOMAIN_MAX).contains(&u);
        prop_assert_eq!(Money::<USD>::try_from_units(u).is_ok(), inside);
    }
    }
}
