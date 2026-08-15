//! The quotient a caller cannot reach without deciding what happens to the residue.

use super::Residue;
use crate::Money;
use crate::StaticCurrency;
use core::marker::PhantomData;

/// A quotient and residue that must be resolved together.
#[must_use = "resolve this division with .take_residue() or .discard_deliberately()"]
pub struct Division<C: StaticCurrency> {
    quotient: i128,
    residue: i128,
    _currency: PhantomData<C>,
}

impl<C: StaticCurrency> Division<C> {
    #[inline]
    pub(crate) const fn new(quotient: i128, residue: i128) -> Self {
        Self { quotient, residue, _currency: PhantomData }
    }

    #[inline]
    const fn quotient(&self) -> Money<C> {
        Money::<C>::from_units_unchecked(self.quotient)
    }

    /// Return the quotient and transfer the residue obligation to the caller.
    #[must_use = "the returned residue is money moved by rounding"]
    pub const fn take_residue(self) -> (Money<C>, Residue<C>) {
        (self.quotient(), Residue::from_units_unchecked(self.residue))
    }

    /// Return the quotient while explicitly accepting loss of the residue.
    #[must_use]
    pub const fn discard_deliberately(self) -> Money<C> {
        self.quotient()
    }

    /// Inspect the residue without resolving the division.
    #[must_use]
    pub const fn residue_units(&self) -> i128 {
        self.residue
    }
}

impl<C: StaticCurrency> core::fmt::Debug for Division<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Division")
            .field("currency", &C::CODE.alpha3())
            .field("quotient", &self.quotient)
            .field("residue", &self.residue)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::iso::USD;
    use crate::{Money, Rounding};
    use core::num::NonZeroU32;

    /// `div_int` returns one value, and there is no way to reach the quotient
    /// without saying what happens to the residue.
    #[test]
    fn a_division_cannot_yield_its_quotient_without_a_decision() {
        let ten = || Money::<USD>::try_from_units(10_000_000_000_000_000_000).unwrap();
        let three = NonZeroU32::new(3).unwrap();

        // Take the residue and post it.
        let division = ten().div_int(three, Rounding::TowardZero);
        assert_eq!(division.residue_units(), 1);
        assert_eq!(
            format!("{division:?}"),
            "Division { currency: \"USD\", quotient: 3333333333333333333, residue: 1 }"
        );
        let (share, residue) = division.take_residue();
        assert_eq!(share.units(), 3_333_333_333_333_333_333);
        assert_eq!(residue.take_units(), 1, "the lost unit is handed back");

        // Or discard it explicitly.
        let share = ten().div_int(three, Rounding::TowardZero).discard_deliberately();
        assert_eq!(share.units(), 3_333_333_333_333_333_333);
    }

    /// Dropping an undecided `Division` is safe.
    ///
    /// No money was handed out because the quotient never escaped.
    #[test]
    fn dropping_an_undecided_division_is_silent_because_nothing_escaped() {
        let m = Money::<USD>::try_from_units(10_000_000_000_000_000_000).unwrap();
        let _ = m.div_int(NonZeroU32::new(3).unwrap(), Rounding::TowardZero);
        // Reaching here without a panic is the assertion.
    }
}
