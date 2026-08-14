//! Conserving distribution over raw canonical units.

use crate::errors::{AllocationError, AmountError};
use crate::rounding::{Rounding, div_round_i256};
use ethnum::I256;

/// Distribute `units` across `weights`, conserving the total exactly.
///
/// The non-generic core of [`Money::allocate`], for callers that only learn the currency at
/// run time. The currency is irrelevant to the arithmetic: this
/// conserves at the canonical scale, which is the only scale money has here.
///
/// A **zero weight** is allowed and receives **exactly zero** — including none of the truncation
/// remainder. A weight of zero is "this recipient has no claim"; handing it a rounding unit
/// would conserve the total while paying the wrong party, which conservation tests cannot see.
/// The remainder is distributed only among positive-weight positions.
///
/// # Errors
/// Returns [`AllocationError::Amount`] when `units` is outside the money domain, or
/// [`AllocationError::InvalidWeights`] when `weights` is empty or all zero.
///
/// # Panics
/// Panics only if an internal remainder or domain invariant is broken.
pub fn allocate_units(units: i128, weights: &[u32]) -> Result<Vec<i128>, AllocationError> {
    if !crate::domain::in_domain(units) {
        return Err(AmountError::out_of_domain(units).into());
    }
    let total_w: i128 = weights.iter().map(|&w| i128::from(w)).sum();
    if weights.is_empty() || total_w == 0 {
        return Err(AllocationError::InvalidWeights { weights: weights.len() });
    }

    let mut parts: Vec<i128> = Vec::with_capacity(weights.len());
    let mut remainder = units;

    for &w in weights {
        // The product can reach ~4.3e45, beyond i128 but far below I256::MAX.
        let num = I256::from(units)
            .checked_mul(I256::from(i128::from(w)))
            .expect("|units * w| <= 4.3e45, ~31 orders of magnitude below I256::MAX");

        // State truncation explicitly. The returned remainder is denominator-relative,
        // while the canonical-unit shortfall is tracked below.
        let (share, _) = div_round_i256(num, I256::from(total_w), Rounding::TowardZero);
        let share = i128::try_from(share).expect("|share| <= |units| <= DOMAIN_MAX");
        parts.push(share);

        // Every share carries the sign of `units` and the partial sum grows monotonically
        // toward it, so `remainder` only ever shrinks: |remainder| <= |units| <= DOMAIN_MAX.
        remainder = remainder
            .checked_sub(share)
            .expect("|remainder| <= |units| <= DOMAIN_MAX, ~170x below i128::MAX");
    }

    // Each positive share loses less than one unit; zero-weight shares lose none. Therefore the
    // remainder is smaller than the number of positive weights.
    let step = remainder.signum();
    let bump = usize::try_from(remainder.unsigned_abs())
        .expect("|remainder| < count of positive weights, which is a usize");
    let positive = weights.iter().filter(|&&w| w != 0).count();
    assert!(
        bump < positive,
        "allocate: {bump} leftover units for {positive} positive-weight parts — conservation is \
         no longer provable",
    );

    // Only positive-weight positions have a claim on the remainder.
    parts.iter_mut().zip(weights).filter(|&(_, &w)| w != 0).take(bump).for_each(|(part, _)| {
        *part = part.checked_add(step).expect("|part| <= |units| <= DOMAIN_MAX, ~170x below i128::MAX");
    });

    Ok(parts)
}

#[cfg(test)]
mod tests {
    use crate::domain::DOMAIN_MAX;
    use crate::errors::{AllocationError, AmountError};

    /// The raw allocator reports both invalid weights and invalid amounts.
    #[test]
    fn the_runtime_allocator_refuses_bad_weights_without_panicking() {
        use crate::arithmetic::allocate_units;

        assert_eq!(allocate_units(1, &[]), Err(AllocationError::InvalidWeights { weights: 0 }));
        assert_eq!(allocate_units(1, &[0, 0]), Err(AllocationError::InvalidWeights { weights: 2 }));
        // The out-of-domain arm remains distinguishable.
        assert_eq!(
            allocate_units(DOMAIN_MAX + 1, &[1, 1]),
            Err(AllocationError::Amount(AmountError::out_of_domain(DOMAIN_MAX + 1)))
        );
        // A single non-zero weight among zeros is allocatable, so the check is "all zero", not "any zero".
        assert_eq!(allocate_units(10, &[0, 1, 0]).unwrap(), vec![0, 10, 0]);
    }
    /// Zero-weight recipients must receive nothing, including truncation remainders. Conservation
    /// alone cannot distinguish `[1, 0, 0]` from `[0, 1, 0]`, so assert the distribution directly.
    #[test]
    fn the_allocator_never_pays_a_zero_weight_recipient() {
        use crate::arithmetic::allocate_units;

        // The odd unit belongs to the first positive-weight slot.
        assert_eq!(allocate_units(1, &[0, 1, 1]).unwrap(), vec![0, 1, 0]);
        assert_eq!(allocate_units(-1, &[0, 1, 1]).unwrap(), vec![0, -1, 0]);
        // Interleaved zeros are skipped too: 7 over weights (3, 3) leaves a 1-unit remainder that
        // must reach the first positive slot (index 1), never the zero at index 0.
        assert_eq!(allocate_units(7, &[0, 3, 0, 3, 0]).unwrap(), vec![0, 4, 0, 3, 0]);

        // The general property, swept: every zero-weight index gets exactly 0, and the total is
        // still conserved, for positive and negative amounts across several zero patterns.
        for units in [1_i128, 7, 100, 999_999, -1, -7, -100, DOMAIN_MAX] {
            for weights in
                [&[0u32, 1, 1][..], &[1, 0, 1], &[1, 1, 0], &[0, 3, 0, 3, 0], &[0, 0, 1], &[7, 0, 0, 2, 0, 1]]
            {
                let parts = allocate_units(units, weights).unwrap();
                for (i, &w) in weights.iter().enumerate() {
                    assert!(
                        w != 0 || parts[i] == 0,
                        "units={units} weights={weights:?}: paid {} to the zero-weight slot at {i}",
                        parts[i]
                    );
                }
                assert_eq!(
                    parts.iter().sum::<i128>(),
                    units,
                    "units={units} weights={weights:?}: not conserved"
                );
            }
        }
    }
}
