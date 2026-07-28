//! The canonical representation.

use crate::currency::StaticCurrency;
use crate::domain::in_domain;
use crate::error::AmountError;
use crate::iso::Iso4217;
use core::marker::PhantomData;

/// A monetary quantity: `units` counts `10^-18` of a currency unit.
///
/// Scale is **fixed at 18 and structural** — it is not a field, so it cannot drift.
/// Invariant: `|units| <= DOMAIN_MAX`. The raw `i128` is never publicly reachable;
/// a caller holding one could reintroduce an unchecked construction path. (DESIGN.md C1)
pub struct Money<C: StaticCurrency> {
    units: i128,
    // The currency lives entirely in the type. `C` is a ZST, so this costs nothing and a
    // `Money<USD>` is exactly an i128. There is no runtime tag because there is no runtime
    // currency — see `currency.rs` for why that variant was deleted. (DESIGN.md C1, C3)
    _c: PhantomData<C>,
}

// Hand-written, NOT derived: `#[derive(Clone)]` emits `impl<C: CurrencyRepr + Clone>`,
// bounding C when the bound belongs on C::Tag. (DESIGN.md E11)
impl<C: StaticCurrency> Clone for Money<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: StaticCurrency> Copy for Money<C> {}
impl<C: StaticCurrency> PartialEq for Money<C> {
    // Only `units`: two `Money<C>` are the same currency BY CONSTRUCTION, so there is nothing
    // else to compare. "Zero dollars is not zero rupiah" became a type error rather than an
    // equality result when the runtime-currency variant was deleted.
    fn eq(&self, o: &Self) -> bool {
        self.units == o.units
    }
}
impl<C: StaticCurrency> Eq for Money<C> {}

// Ordering and hashing read `units` alone, exactly as `PartialEq` does, so `Hash` agrees with
// `Eq` and `Ord` agrees with both — the consistency the standard library requires of anything
// used as a map key or sorted.
//
// There is NO cross-currency question here, and that is the whole reason these are safe to
// provide. `Money<USD>` and `Money<IDR>` are different types, so `a < b` can only ever compare
// two amounts of the same currency; the comparison is total and means what it looks like.
// (DESIGN.md F2 leaves ordering open for the SQL type, where a column CAN hold mixed
// currencies and "which is larger" genuinely has no answer. That question does not reach
// Rust, and the two must not be conflated.)
//
// Hand-written rather than derived for the reason given above `Clone`: a derive would bound
// `C: Ord`/`C: Hash`, and `C` is a ZST marker that need not be either.
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
    /// Takes `i128` rather than a narrower integer because C10 says accept FROM lower precision
    /// freely and never narrow a parameter. Callers holding an `i8`/`i16`/`i32`/`i64` widen for
    /// free with `i128::from(x)`.
    ///
    /// At `SCALE = 12` there was a second, sharper reason: the domain permitted ~1e24 major
    /// units, so an `i64` parameter left the top 5.04 orders unreachable and the `Option` could
    /// never be `None`. At `SCALE = 18` the major range is ~1e18, which `i64::MAX` covers
    /// entirely, so that argument no longer applies — a reminder that a bound derived from a
    /// constant silently stops being true when the constant moves.
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
    use super::*;
    use crate::domain::DOMAIN_MAX;
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

    /// The compile-time currency is FREE: a `Money<USD>` is exactly an `i128`.
    ///
    /// DESIGN.md E8 measured `Money<Dyn>` at 32 bytes — double, because `i128`'s 16-byte
    /// alignment padded a 2-byte tag out to a full 16. That variant is gone, so the 2x is no
    /// longer a cost anyone pays. This pins the property that outlived it.
    #[test]
    fn the_compile_time_currency_costs_nothing() {
        assert_eq!(size_of::<Money<USD>>(), 16);
        assert_eq!(size_of::<Money<USD>>(), size_of::<i128>());
    }

    /// 10.5 and 10.500 are literally the same `i128` — normalization is not hard to get right
    /// here, it does not exist. Demonstrating that requires reaching the value by DIFFERENT
    /// routes; an earlier version of this test built it twice the same way and asserted
    /// equality, which any `Eq` impl passes. (DESIGN.md C1)
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
        assert_eq!(Money::<USD>::try_from_major(10).unwrap().units(), 10 * crate::domain::POW10_SCALE);
        assert_eq!(Money::<USD>::try_from_major(-3).unwrap().units(), -3 * crate::domain::POW10_SCALE);
        assert_eq!(Money::<USD>::ZERO.units(), 0);
    }

    /// Zero dollars is NOT zero rupiah — and that is now a TYPE error, not an assertion.
    ///
    /// This used to compare two `Money<Dyn>` and check that equality included the currency.
    /// With the runtime variant gone, `Money::<USD>::ZERO == Money::<IDR>::ZERO` does not
    /// compile at all, which is the stronger form of the same claim and is pinned by
    /// `tests/ui/cross_currency_add`. What is left to test here is the currency-blind
    /// question, which still has its own answer.
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
        // DERIVED from the constants, never a literal, precisely so it survives a scale change.
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
}
