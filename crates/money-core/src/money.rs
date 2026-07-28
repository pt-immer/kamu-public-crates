//! The canonical representation.

use crate::currency::StaticCurrency;
use crate::domain::in_domain;
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

    /// Construct from canonical units, enforcing the domain.
    #[inline]
    #[must_use]
    pub const fn from_units(units: i128) -> Option<Self> {
        if in_domain(units) { Some(Self { units, _c: PhantomData }) } else { None }
    }

    /// Construct from whole currency units (e.g. `from_major(10)` is 10.000000000000000000).
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
    /// Returns `None` when `major` lies outside the domain measured in major units. That arm is
    /// reachable and is pinned by `from_major_spans_the_domain_and_rejects_beyond_it`.
    #[inline]
    #[must_use]
    pub const fn from_major(major: i128) -> Option<Self> {
        match major.checked_mul(crate::domain::POW10_SCALE) {
            Some(u) => Self::from_units(u),
            None => None,
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
        assert!(Money::<USD>::from_units(0).is_some());
        assert!(Money::<USD>::from_units(DOMAIN_MAX).is_some());
        assert!(Money::<USD>::from_units(-DOMAIN_MAX).is_some());
        assert!(Money::<USD>::from_units(DOMAIN_MAX + 1).is_none());
        assert!(Money::<USD>::from_units(-DOMAIN_MAX - 1).is_none());
        assert!(Money::<USD>::from_units(i128::MIN).is_none(), "i128::MIN must not sneak in");
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
        let direct = Money::<USD>::from_units(10_500_000_000_000_000_000).unwrap();
        let built = Money::<USD>::from_major(10).unwrap()
            + Money::<USD>::from_units(500_000_000_000_000_000).unwrap();
        assert_eq!(direct, built);
        assert_eq!(direct.units(), built.units());
    }

    #[test]
    fn code_comes_from_the_type() {
        assert_eq!(Money::<USD>::from_units(1).unwrap().code(), Iso4217::USD);
        assert_eq!(Money::<IDR>::from_units(1).unwrap().code(), Iso4217::IDR);
    }

    #[test]
    fn from_major_scales_by_pow10() {
        assert_eq!(Money::<USD>::from_major(10).unwrap().units(), 10 * crate::domain::POW10_SCALE);
        assert_eq!(Money::<USD>::from_major(-3).unwrap().units(), -3 * crate::domain::POW10_SCALE);
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
        assert!(!Money::<USD>::from_units(1).unwrap().is_zero());
    }

    /// `from_major`'s `Option` is REACHABLE, and the constructor spans the whole domain.
    ///
    /// HISTORY, because it reversed inside one day and the reversal is instructive. An earlier
    /// signature took `i64`. At `SCALE = 12` the domain permitted ~1e24 major units, so `i64`
    /// left 5.04 orders unreachable and the `None` arm could never be taken — and the doc
    /// presented that totality as a virtue when it was totality bought by amputating range.
    /// At `SCALE = 18` the major range is only ~1e18, which `i64::MAX` (~9.2e18) fully covers,
    /// so that specific argument no longer holds. `i128` is still correct, for a different and
    /// better reason: C10 says accept FROM lower precision freely and never narrow a parameter.
    ///
    /// The lesson is not about `i64`. It is that a bound derived from one constant becomes
    /// wrong the moment that constant moves, and nothing tells you — the code still compiles
    /// and the doc still reads plausibly.
    ///
    /// Mutation-check: force `from_major` to always return `Some`; this test must go red.
    #[test]
    fn from_major_spans_the_domain_and_rejects_beyond_it() {
        // DERIVED from the constants, never a literal, precisely so it survives a scale change.
        const MAX_MAJOR: i128 = DOMAIN_MAX / crate::domain::POW10_SCALE; // 10^18 - 1 at SCALE 18

        assert!(Money::<USD>::from_major(MAX_MAJOR).is_some(), "top of the domain");
        assert!(Money::<USD>::from_major(-MAX_MAJOR).is_some(), "bottom of the domain");
        assert!(
            Money::<USD>::from_major(MAX_MAJOR + 1).is_none(),
            "one major unit above the domain must be refused"
        );
        assert!(
            Money::<USD>::from_major(-MAX_MAJOR - 1).is_none(),
            "one major unit below the domain must be refused"
        );

        // The whole major range now fits an i64, so there is no "range i64 cannot reach" to
        // assert. Pin the relationship that replaced it instead: MAX_MAJOR is derived from
        // SCALE, so this stays true at any scale and would have caught the 12 -> 18 move.
        assert_eq!(
            MAX_MAJOR,
            10i128.pow(crate::domain::PRECISION - crate::domain::SCALE) - 1,
            "major range is 10^(PRECISION-SCALE) - 1, whatever SCALE happens to be"
        );
        assert!(Money::<USD>::from_major(0).is_some());
    }
}
