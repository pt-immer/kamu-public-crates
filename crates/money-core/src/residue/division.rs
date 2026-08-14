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
