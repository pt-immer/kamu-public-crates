//! The domain: what a Money is allowed to be.

/// Fractional digits shared by money and rates.
///
/// Scale is structural rather than stored per value, so it cannot drift.
pub const SCALE: u32 = 18;

/// Total digits. The `36` in `NUMERIC(36,18)`.
///
/// Named separately from [`SCALE`] so the domain derivation has no magic
/// literals.
pub const PRECISION: u32 = 36;

/// `10^SCALE`. One whole currency unit, expressed in canonical units.
pub const POW10_SCALE: i128 = 10i128.pow(SCALE);

/// Largest representable magnitude, in canonical units.
///
/// `NUMERIC(36,18)` admits `|v| < 10^18` with 18 fractional digits, i.e. `< 10^36` units.
/// `i128::MAX` leaves more than 100 times this magnitude, so two valid
/// operands can be combined before the result is range-checked. Sums use an
/// `I256` accumulator and narrow only after the final total is known.
pub const DOMAIN_MAX: i128 = 10i128.pow(PRECISION) - 1;

// The checking margin is a compile-time property of these constants, so enforce it at
// compile time: editing DOMAIN_MAX to break the invariant fails the BUILD, not a test run
// that might never be executed. This is what lets every operation compute-then-check
// instead of pre-checking, so it must never silently regress.
const _: () = assert!(DOMAIN_MAX < i128::MAX);
const _: () = assert!(DOMAIN_MAX.checked_add(DOMAIN_MAX).is_some());
const _: () = assert!(i128::MAX / DOMAIN_MAX >= 100);

/// `true` iff `units` is inside the domain.
#[must_use]
#[inline]
pub const fn in_domain(units: i128) -> bool {
    units <= DOMAIN_MAX && units >= -DOMAIN_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_max_is_the_numeric_36_18_bound() {
        // 18 fractional digits within 36 total => integer part < 10^18, whole domain 10^36
        // units. DOMAIN_MAX counts units, so it is the same 10^36 - 1 it was at scale 12.
        assert_eq!(DOMAIN_MAX, 10i128.pow(PRECISION) - 1);
    }

    #[test]
    fn schema_literals_are_pinned() {
        assert_eq!(PRECISION, 36, "the 36 in NUMERIC(36,18)");
        assert_eq!(SCALE, 18, "the 18 in NUMERIC(36,18)");
        assert_eq!(POW10_SCALE, 1_000_000_000_000_000_000);
    }

    /// Verifies the derivation rather than restating a definition: the integer part spans
    /// `10^(PRECISION - SCALE)`, so in units of `10^-SCALE` the whole domain is
    /// `10^(PRECISION-SCALE) * 10^SCALE = 10^PRECISION`. This ties `DOMAIN_MAX`, `PRECISION`,
    /// `SCALE` and `POW10_SCALE` together.
    #[test]
    fn domain_max_derives_from_the_pg_stated_bound() {
        let pg_integer_bound = 10i128.pow(PRECISION - SCALE);
        assert_eq!(pg_integer_bound, 10i128.pow(18));
        assert_eq!(DOMAIN_MAX + 1, pg_integer_bound * POW10_SCALE);
    }

    #[test]
    fn domain_boundary_is_inclusive_at_the_max() {
        assert!(in_domain(0));
        assert!(in_domain(DOMAIN_MAX));
        assert!(in_domain(-DOMAIN_MAX));
        assert!(!in_domain(DOMAIN_MAX + 1));
        assert!(!in_domain(-DOMAIN_MAX - 1));
        assert!(!in_domain(i128::MAX));
        assert!(!in_domain(i128::MIN));
    }
}
