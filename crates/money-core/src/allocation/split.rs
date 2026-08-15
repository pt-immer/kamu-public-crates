//! Equal division into near-equal parts, lazily and without allocating.

use crate::Money;
use crate::StaticCurrency;
use core::marker::PhantomData;
use core::num::NonZeroU32;
use std::collections::TryReserveError;

impl<C: StaticCurrency> Money<C> {
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

/// The lazy half of [`Money::split`](crate::Money::split), returned by [`Money::split`](crate::Money::split).
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

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::domain::DOMAIN_MAX;
    use crate::iso::USD;

    fn m(u: i128) -> Money<USD> {
        Money::<USD>::try_from_units(u).unwrap()
    }

    #[test]
    fn split_conserves() {
        use core::num::NonZeroU32;
        let parts: Vec<_> = m(10_000_000_000_000_000_000).split(NonZeroU32::new(3).unwrap()).collect();
        assert_eq!(parts.iter().map(Money::units).sum::<i128>(), 10_000_000_000_000_000_000);
    }
    /// Lazy equal splitting must match equal-weight allocation across signs and remainders.
    #[test]
    fn split_matches_the_allocation_it_replaced() {
        use core::num::NonZeroU32;
        for units in [
            0,
            1,
            -1,
            7,
            -7,
            10_000_000_000_000_000_000,
            -10_000_000_000_000_000_000,
            DOMAIN_MAX,
            -DOMAIN_MAX,
        ] {
            for n in [1u32, 2, 3, 4, 7, 10, 97] {
                let count = NonZeroU32::new(n).unwrap();
                let split: Vec<_> = m(units).split(count).collect();
                let allocated = m(units)
                    .allocate(&vec![1u32; usize::try_from(n).expect("a u32 part count fits usize")])
                    .unwrap();
                assert_eq!(split, allocated, "units={units} n={n}");

                assert_eq!(
                    split.len(),
                    usize::try_from(n).expect("a u32 part count fits usize"),
                    "part count"
                );
                assert_eq!(
                    split.iter().map(Money::units).sum::<i128>(),
                    units,
                    "conservation: units={units} n={n}"
                );
            }
        }
    }
    /// The lazy and fallible collection paths must preserve one distribution.
    #[test]
    fn split_collection_paths_preserve_one_distribution() {
        use core::num::NonZeroU32;
        for units in [0, 1, -1, 7, -7, DOMAIN_MAX, -DOMAIN_MAX] {
            for n in [1u32, 2, 3, 7, 97] {
                let count = NonZeroU32::new(n).unwrap();
                let fallible = m(units).split_collect(count).expect("97 parts always fit");
                let lazy: Vec<_> = m(units).split(count).collect();

                assert_eq!(fallible, lazy, "units={units} n={n}: split_collect diverged");
                assert_eq!(
                    lazy.iter().map(Money::units).sum::<i128>(),
                    units,
                    "conservation through the lazy path: units={units} n={n}"
                );
            }
        }
    }
    /// `NonZeroU32` admits `u32::MAX`; an eager implementation could reserve about 68.7 GB for that
    /// count. The iterator remains constant-size, reports the exact count, and yields initial
    /// parts without allocating.
    ///
    /// **It deliberately does not try to make `split_collect` fail.** A real 68.7 GB `try_reserve_exact`
    /// is not a reliable assertion: under Linux overcommit the reservation can succeed and hand back
    /// address space, and the failure then arrives as the OOM killer partway through filling, which
    /// is not a test result — it is a dead test runner. Instead, prove that the lazy path never
    /// makes the request.
    #[test]
    fn split_costs_nothing_at_the_part_count_that_motivated_it() {
        use core::num::NonZeroU32;
        let all = usize::try_from(u32::MAX).expect("64-bit target");
        let mut parts = m(DOMAIN_MAX).split(NonZeroU32::new(u32::MAX).unwrap());

        assert_eq!(parts.len(), all, "exact size, with nothing materialised");

        // This iterator yields by value, while `Money::units` takes a reference.
        let head: Vec<i128> = parts.by_ref().take(3).map(|p| p.units()).collect();
        assert_eq!(head.len(), 3);
        assert_eq!(
            parts.len(),
            all.saturating_sub(3),
            "the iterator reports what is left, so split_collect can reserve exactly once"
        );

        // Every part is in domain and they descend by at most one unit, which is the same shape the
        // eager path produces -- checked here on the first few rather than on all 4.29 billion.
        let base = DOMAIN_MAX / i128::from(u32::MAX);
        for (i, units) in head.iter().enumerate() {
            assert!(
                *units == base || *units == base + 1,
                "part {i} is {units}, expected {base} or {}",
                base + 1
            );
        }
    }
    /// The parts differ by at most one unit — the property that makes it a *split* rather than
    /// an arbitrary conserving distribution.
    #[test]
    fn split_parts_differ_by_at_most_one_unit() {
        use core::num::NonZeroU32;
        for units in [DOMAIN_MAX, -DOMAIN_MAX, 1, -1, 0, 12_345] {
            for n in [1u32, 3, 8, 101] {
                let parts: Vec<_> = m(units).split(NonZeroU32::new(n).unwrap()).collect();
                let max = parts.iter().map(Money::units).max().unwrap();
                let min = parts.iter().map(Money::units).min().unwrap();
                assert!(max - min <= 1, "units={units} n={n}: spread {} exceeds one unit", max - min);
            }
        }
    }
}
