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
