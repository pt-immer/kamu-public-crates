//! Canonical units moved by rounding: an accounting obligation, not a runtime trap.

use crate::StaticCurrency;
use crate::errors::AmountError;
use crate::iso::Iso4217;
use core::marker::PhantomData;

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
        if crate::domain::in_domain(units) {
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

#[cfg(test)]
mod tests {
    use crate::Residue;
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AmountError;
    use crate::iso::USD;

    #[test]
    fn residue_constructor_enforces_the_amount_domain() {
        let residue = Residue::<USD>::try_from_units(7).unwrap();
        assert_eq!(residue.units(), 7);
        assert_eq!(residue.code().alpha3(), "USD");
        assert_eq!(format!("{residue:?}"), "Residue { currency: \"USD\", units: 7 }");

        assert_eq!(
            Residue::<USD>::try_from_units(DOMAIN_MAX + 1).unwrap_err(),
            AmountError::out_of_domain(DOMAIN_MAX + 1)
        );
        assert_eq!(
            Residue::<USD>::try_from_units(-DOMAIN_MAX - 1).unwrap_err(),
            AmountError::out_of_domain(-DOMAIN_MAX - 1)
        );
    }

    #[test]
    fn discard_deliberately_is_silent() {
        Residue::<USD>::try_from_units(7).unwrap().discard_deliberately();
    }

    #[test]
    fn take_units_absorbs() {
        let r = Residue::<USD>::try_from_units(7).unwrap();
        assert_eq!(r.take_units(), 7);
    }

    #[test]
    fn dropping_a_residue_never_panics() {
        drop(Residue::<USD>::try_from_units(1).unwrap());
    }
}
