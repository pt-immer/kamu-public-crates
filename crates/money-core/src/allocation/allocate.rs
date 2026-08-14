//! Weighted distribution that conserves the total exactly.

use crate::Money;
use crate::StaticCurrency;
use crate::arithmetic::allocate_units;
use crate::errors::AllocationError;

impl<C: StaticCurrency> Money<C> {
    /// Distribute this amount across `weights`, preserving the total exactly.
    ///
    /// Zero-weight positions receive zero. Any rounding remainder goes to positive-weight
    /// positions from the front.
    ///
    /// # Errors
    /// Returns [`AllocationError::InvalidWeights`] when `weights` is empty or all zero.
    ///
    /// # Panics
    /// Panics only if the internal allocation kernel produces an out-of-domain part.
    pub fn allocate(self, weights: &[u32]) -> Result<Vec<Self>, AllocationError> {
        Ok(allocate_units(self.units(), weights)?
            .into_iter()
            .map(|u| Self::try_from_units(u).expect("|part| <= |whole| <= DOMAIN_MAX"))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AllocationError;
    use crate::iso::USD;

    fn m(u: i128) -> Money<USD> {
        Money::<USD>::try_from_units(u).unwrap()
    }

    #[test]
    fn allocate_conserves_the_whole() {
        let parts = m(10_000_000_000_000_000_000).allocate(&[1, 1, 1]).unwrap();
        assert_eq!(parts.len(), 3);
        let sum: i128 = parts.iter().map(Money::units).sum();
        assert_eq!(sum, 10_000_000_000_000_000_000, "a naive split would lose a unit here");
        assert_eq!(parts[0].units(), 3_333_333_333_333_333_334, "the remainder lands on the first part");
    }
    #[test]
    fn allocate_respects_weights() {
        let parts = m(10_000_000_000_000_000_000).allocate(&[3, 7]).unwrap();
        assert_eq!(parts[0].units(), 3_000_000_000_000_000_000);
        assert_eq!(parts[1].units(), 7_000_000_000_000_000_000);
    }
    #[test]
    fn allocate_at_the_domain_top_does_not_overflow() {
        // units * weight is about 4e45, beyond i128; the wide path must carry it.
        let parts = m(DOMAIN_MAX).allocate(&[u32::MAX, 1]).unwrap();
        let sum: i128 = parts.iter().map(Money::units).sum();
        assert_eq!(sum, DOMAIN_MAX);
    }
    #[test]
    fn allocate_handles_negative_and_tiny() {
        assert_eq!(m(1).allocate(&[1, 1, 1]).unwrap().iter().map(Money::units).sum::<i128>(), 1);
        assert_eq!(
            m(-10_000_000_000_000_000_000)
                .allocate(&[1, 1, 1])
                .unwrap()
                .iter()
                .map(Money::units)
                .sum::<i128>(),
            -10_000_000_000_000_000_000
        );
    }
    /// The typed allocation path reports invalid runtime weights without exposing raw units.
    #[test]
    fn allocate_reports_bad_weights_instead_of_panicking() {
        assert_eq!(
            m(100).allocate(&[]),
            Err(AllocationError::InvalidWeights { weights: 0 }),
            "empty weights are a value, not a panic"
        );
        assert_eq!(
            m(100).allocate(&[0, 0, 0]),
            Err(AllocationError::InvalidWeights { weights: 3 }),
            "all-zero weights have no meaningful distribution"
        );

        // The typed facade must preserve the raw kernel's distribution.
        for units in [0, 1, -1, 7, -7, DOMAIN_MAX, -DOMAIN_MAX] {
            for weights in [&[1u32, 1, 1][..], &[3, 1][..], &[1, 0, 2][..], &[5][..]] {
                let typed = m(units).allocate(weights).expect("weights are valid");
                let raw = crate::arithmetic::allocate_units(units, weights).unwrap();
                assert_eq!(typed.iter().map(Money::units).collect::<Vec<_>>(), raw);
            }
        }
    }

    use proptest::prelude::*;

    proptest! {
        /// Allocation never creates or destroys money.
        #[test]
        fn prop_allocate_always_conserves(
            units in -DOMAIN_MAX..=DOMAIN_MAX,
            weights in prop::collection::vec(1u32..=1_000_000, 1..12),
        ) {
            let parts = m(units).allocate(&weights).unwrap();
            prop_assert_eq!(parts.len(), weights.len());
            let sum: i128 = parts.iter().map(Money::units).sum();
            prop_assert_eq!(sum, units);
        }
    }
}
