//! The canonical representation.

use crate::StaticCurrency;
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

mod compare;
mod construct;
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

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::iso::{IDR, Iso4217, USD};

    /// The compile-time currency is zero-sized.
    #[test]
    fn the_compile_time_currency_costs_nothing() {
        assert_eq!(size_of::<Money<USD>>(), 16);
        assert_eq!(size_of::<Money<USD>>(), size_of::<i128>());
    }
    #[test]
    fn code_comes_from_the_type() {
        assert_eq!(Money::<USD>::try_from_units(1).unwrap().code(), Iso4217::USD);
        assert_eq!(Money::<IDR>::try_from_units(1).unwrap().code(), Iso4217::IDR);
    }
    /// `is_zero` inspects magnitude; the generic type retains currency identity.
    #[test]
    fn is_zero_asks_only_about_magnitude() {
        assert!(Money::<USD>::ZERO.is_zero());
        assert!(Money::<IDR>::ZERO.is_zero());
        assert!(!Money::<USD>::try_from_units(1).unwrap().is_zero());
    }
}
