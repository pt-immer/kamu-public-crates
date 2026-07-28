//! Explicit accounting for units moved by integer division.
//!
//! [`Division`] keeps quotient and residue together. A caller cannot obtain the
//! quotient without choosing [`Division::take_residue`] or
//! [`Division::discard_deliberately`]. Dropping an undecided division is safe:
//! no quotient escaped.
//!
//! [`Residue`] is a `#[must_use]` accounting obligation, not a runtime trap.
//! Rust does not provide linear types, so code can still suppress the lint and
//! drop a bare residue. The API makes that choice visible without introducing a
//! panic in `Drop`, including during cancellation or unwinding.

use crate::Money;
use crate::StaticCurrency;
use crate::error_impl::AmountError;
use crate::iso::Iso4217;
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

/// Runtime-currency division result for adapters.
///
/// Prefer [`Division`] in typed Rust code. This form exposes raw units because
/// adapters such as a PostgreSQL extension learn currency identity at runtime.
#[must_use = "resolve this division with .take_residue() or .discard_deliberately()"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UntaggedDivision {
    quotient: i128,
    residue: i128,
}

impl UntaggedDivision {
    #[inline]
    pub(crate) const fn new(quotient: i128, residue: i128) -> Self {
        Self { quotient, residue }
    }

    /// Return both unit counts and transfer accounting responsibility.
    #[must_use]
    pub const fn take_residue(self) -> (i128, i128) {
        (self.quotient, self.residue)
    }

    /// Return the quotient while explicitly accepting loss of the residue.
    #[must_use]
    pub const fn discard_deliberately(self) -> i128 {
        self.quotient
    }

    /// Inspect the residue without resolving the division.
    #[must_use]
    pub const fn residue_units(&self) -> i128 {
        self.residue
    }
}

/// Canonical units moved by a rounding operation.
///
/// Consume with [`Self::take_units`] and post the units, or call
/// [`Self::discard_deliberately`] to record intentional loss.
#[must_use = "post these residue units or call .discard_deliberately()"]
pub struct Residue<C: StaticCurrency> {
    units: i128,
    _currency: PhantomData<C>,
}

impl<C: StaticCurrency> Residue<C> {
    /// Construct a residue from canonical units.
    ///
    /// # Errors
    ///
    /// Returns [`AmountError`] outside the fixed money domain.
    pub const fn try_from_units(units: i128) -> Result<Self, AmountError> {
        if crate::domain_impl::in_domain(units) {
            Ok(Self::from_units_unchecked(units))
        } else {
            Err(AmountError::out_of_domain(units))
        }
    }

    #[inline]
    pub(crate) const fn from_units_unchecked(units: i128) -> Self {
        Self { units, _currency: PhantomData }
    }

    /// Inspect the residue without consuming it.
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }

    /// Return the residue currency.
    #[must_use]
    pub const fn code(&self) -> Iso4217 {
        C::CODE
    }

    /// Consume the residue and return units for posting.
    #[must_use = "post these units in the ledger"]
    pub const fn take_units(self) -> i128 {
        self.units
    }

    /// Consume the residue while explicitly accepting its loss.
    pub const fn discard_deliberately(self) {}
}

impl<C: StaticCurrency> core::fmt::Debug for Residue<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Residue").field("currency", &C::CODE.alpha3()).field("units", &self.units).finish()
    }
}
