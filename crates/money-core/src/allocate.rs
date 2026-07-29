//! Conservative distribution: splitting money without losing any.

use crate::Money;
use crate::StaticCurrency;
use crate::error_impl::{AllocationError, AmountError};
use crate::rounding_impl::{Rounding, div_round_i256};
use core::marker::PhantomData;
use core::num::NonZeroU32;
use ethnum::I256;
use std::collections::TryReserveError;

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
    if !crate::domain_impl::in_domain(units) {
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

    /// Collect [`split`](Self::split) into a pre-reserved vector.
    ///
    /// Bound request-derived counts before calling this method. A successful reservation does
    /// not guarantee that overcommitted memory remains available while the vector is filled.
    ///
    /// # Errors
    /// Returns [`TryReserveError`] when the capacity overflows or the allocator refuses it.
    ///
    /// # Panics
    /// Panics when `n` cannot fit in `usize` on this target.
    pub fn split_collect(self, n: NonZeroU32) -> Result<Vec<Self>, TryReserveError> {
        let parts = self.split(n);
        let mut out = Vec::new();
        out.try_reserve_exact(parts.len())?;
        out.extend(parts);
        Ok(out)
    }

    /// Split into `n` near-equal parts lazily, preserving the total exactly.
    ///
    /// Returns an allocation-free [`ExactSizeIterator`]. Leading parts receive any one-unit
    /// remainder, matching equal-weight [`allocate`](Self::allocate).
    ///
    /// # Panics
    /// Panics when `n` cannot fit in `usize` on this target.
    #[must_use]
    pub fn split(self, n: NonZeroU32) -> SplitParts<C> {
        let count = usize::try_from(n.get()).expect("part count exceeds this target's usize");
        let units = self.units();
        let divisor = i128::from(n.get());

        // Truncate toward zero to match equal-weight `allocate_units`, including negative values.
        let base = units.checked_div(divisor).expect("divisor came from a NonZeroU32");
        let remainder = units.checked_rem(divisor).expect("divisor came from a NonZeroU32");

        // `%` takes the sign of the dividend, so a negative amount distributes negative units.
        // The front parts absorb them, one each, exactly as `allocate_units` does.
        let step = if remainder < 0 { -1 } else { 1 };
        let extra =
            usize::try_from(remainder.unsigned_abs()).expect("|units % n| < n, and n came from a u32");

        SplitParts { base, step, extra, next: 0, count, _currency: PhantomData }
    }
}

/// The lazy half of [`Money::split`], returned by [`Money::split`].
///
/// Holds O(1) state and allocates nothing; large counts cost iteration time, not memory.
pub struct SplitParts<C: StaticCurrency> {
    base: i128,
    /// `+1` or `-1`: the direction the truncation remainder is handed out in, which follows the
    /// sign of the amount rather than of the divisor.
    step: i128,
    /// How many leading parts receive one extra unit.
    extra: usize,
    next: usize,
    count: usize,
    // The iterator state is currency-agnostic; the marker binds its output currency.
    _currency: PhantomData<C>,
}

// Not Copy: consuming a copied cursor could replay values. A manual Clone avoids a needless
// `C: Clone` bound from derive.
impl<C: StaticCurrency> Clone for SplitParts<C> {
    fn clone(&self) -> Self {
        Self {
            base: self.base,
            step: self.step,
            extra: self.extra,
            next: self.next,
            count: self.count,
            _currency: PhantomData,
        }
    }
}

impl<C: StaticCurrency> core::fmt::Debug for SplitParts<C> {
    /// Report progress; distribution constants are derived internals.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SplitParts")
            .field("currency", &C::CODE.alpha3())
            .field("remaining", &self.len())
            .field("of", &self.count)
            .finish_non_exhaustive()
    }
}

impl<C: StaticCurrency> Iterator for SplitParts<C> {
    type Item = Money<C>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let units = if self.next < self.extra {
            self.base.checked_add(self.step).expect("|part| <= |whole| <= DOMAIN_MAX")
        } else {
            self.base
        };
        // `saturating_add` rather than `+`: `clippy::arithmetic_side_effects` is denied
        // crate-wide. It cannot saturate here — the guard above bounds `next` by `count`.
        self.next = self.next.saturating_add(1);
        Some(Money::try_from_units(units).expect("|part| <= |whole| <= DOMAIN_MAX"))
    }

    /// Exact, which is what lets [`Money::split_collect`] reserve once and correctly.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.count.saturating_sub(self.next);
        (remaining, Some(remaining))
    }
}

impl<C: StaticCurrency> ExactSizeIterator for SplitParts<C> {}
impl<C: StaticCurrency> core::iter::FusedIterator for SplitParts<C> {}
