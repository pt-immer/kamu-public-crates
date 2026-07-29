//! The canonical representation.

use crate::StaticCurrency;
use crate::domain_impl::in_domain;
use crate::error_impl::AmountError;
use crate::iso::Iso4217;
use core::marker::PhantomData;

/// A monetary quantity: `units` counts `10^-18` of a currency unit.
///
/// Scale is **fixed at 18 and structural** — it is not a field, so it cannot drift.
/// Invariant: `|units| <= DOMAIN_MAX`. Raw units are read-only; reconstruction
/// requires a checked constructor.
pub struct Money<C: StaticCurrency> {
    units: i128,
    // The currency marker is zero-sized, so Money<C> has the width of i128.
    _c: PhantomData<C>,
}

// Hand-written to avoid adding unnecessary trait bounds to the marker type.
impl<C: StaticCurrency> Clone for Money<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: StaticCurrency> Copy for Money<C> {}
impl<C: StaticCurrency> PartialEq for Money<C> {
    // Two Money<C> values have the same currency by construction.
    fn eq(&self, o: &Self) -> bool {
        self.units == o.units
    }
}
impl<C: StaticCurrency> Eq for Money<C> {}

// Ordering and hashing use units alone. Cross-currency comparison cannot type-check.
// Manual impls avoid unnecessary Ord/Hash bounds on the marker.
impl<C: StaticCurrency> PartialOrd for Money<C> {
    fn partial_cmp(&self, o: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl<C: StaticCurrency> Ord for Money<C> {
    fn cmp(&self, o: &Self) -> core::cmp::Ordering {
        self.units.cmp(&o.units)
    }
}
impl<C: StaticCurrency> core::hash::Hash for Money<C> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.units.hash(state);
    }
}

/// Zero, which is the only amount that means the same thing in every currency.
///
/// Provided so a struct holding a `Money<C>` can `#[derive(Default)]`, and so
/// `unwrap_or_default` and `entry().or_default()` work. [`Money::ZERO`] is the explicit
/// spelling and remains the one to prefer in code a human reads.
impl<C: StaticCurrency> Default for Money<C> {
    fn default() -> Self {
        Self::ZERO
    }
}
impl<C: StaticCurrency> core::fmt::Debug for Money<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Money({} units, {})", self.units, self.code().alpha3())
    }
}

impl<C: StaticCurrency> Money<C> {
    /// The currency of this value. Always `C::CODE` — it cannot be anything else.
    #[inline]
    #[must_use]
    pub const fn code(&self) -> Iso4217 {
        C::CODE
    }

    /// The canonical units. Read-only: reconstructing requires a checked constructor.
    #[inline]
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }

    /// `true` iff this is exactly zero. Sign-agnostic; there is no negative zero.
    #[inline]
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.units == 0
    }
}

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
        match major.checked_mul(crate::domain_impl::POW10_SCALE) {
            Some(units) => Self::try_from_units(units),
            None => Err(AmountError::MajorScaleOverflow { attempted_major: major }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_impl::DOMAIN_MAX;
    use crate::iso::{IDR, Iso4217, USD};

    #[test]
    fn construction_enforces_the_domain() {
        assert!(Money::<USD>::try_from_units(0).is_ok());
        assert!(Money::<USD>::try_from_units(DOMAIN_MAX).is_ok());
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX).is_ok());
        assert!(Money::<USD>::try_from_units(DOMAIN_MAX + 1).is_err());
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX - 1).is_err());
        assert!(Money::<USD>::try_from_units(i128::MIN).is_err(), "i128::MIN must not sneak in");
    }

    /// The compile-time currency is zero-sized.
    #[test]
    fn the_compile_time_currency_costs_nothing() {
        assert_eq!(size_of::<Money<USD>>(), 16);
        assert_eq!(size_of::<Money<USD>>(), size_of::<i128>());
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
    fn code_comes_from_the_type() {
        assert_eq!(Money::<USD>::try_from_units(1).unwrap().code(), Iso4217::USD);
        assert_eq!(Money::<IDR>::try_from_units(1).unwrap().code(), Iso4217::IDR);
    }

    #[test]
    fn from_major_scales_by_pow10() {
        assert_eq!(Money::<USD>::try_from_major(10).unwrap().units(), 10 * crate::domain_impl::POW10_SCALE);
        assert_eq!(Money::<USD>::try_from_major(-3).unwrap().units(), -3 * crate::domain_impl::POW10_SCALE);
        assert_eq!(Money::<USD>::ZERO.units(), 0);
    }

    /// `is_zero` inspects magnitude; the generic type retains currency identity.
    #[test]
    fn is_zero_asks_only_about_magnitude() {
        assert!(Money::<USD>::ZERO.is_zero());
        assert!(Money::<IDR>::ZERO.is_zero());
        assert!(!Money::<USD>::try_from_units(1).unwrap().is_zero());
    }

    /// The constructor spans the domain and distinguishes domain rejection from scaling
    /// overflow.
    #[test]
    fn from_major_spans_the_domain_and_rejects_beyond_it() {
        // Derived from the domain constants so the test follows a scale change.
        const MAX_MAJOR: i128 = DOMAIN_MAX / crate::domain_impl::POW10_SCALE; // 10^18 - 1 at SCALE 18

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
            10i128.pow(crate::domain_impl::PRECISION - crate::domain_impl::SCALE) - 1,
            "major range is 10^(PRECISION-SCALE) - 1, whatever SCALE happens to be"
        );
        assert!(Money::<USD>::try_from_major(0).is_ok());
    }
}
