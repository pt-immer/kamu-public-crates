//! Equality, ordering and hashing, all by canonical units alone. Cross-currency comparison
//! cannot type-check, so none of these needs to ask about the currency.

use super::Money;
use crate::StaticCurrency;

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
