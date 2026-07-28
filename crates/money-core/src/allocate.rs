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
/// run time — a PostgreSQL type cannot be generic, and C9 requires the adapter to share this
/// implementation rather than restate it. The currency is irrelevant to the arithmetic: this
/// conserves at the canonical scale, which is the only scale money has here.
///
/// A **zero weight** is allowed and receives **exactly zero** — including none of the truncation
/// remainder. A weight of zero is "this recipient has no claim"; handing it a rounding unit
/// would conserve the total while paying the wrong party, which conservation tests cannot see.
/// So the remainder is distributed only among positive-weight positions (R2-F1).
///
/// # Errors
/// [`AllocationError::Amount`] if `units` is outside the domain. Without it this function
/// accepted `i128::MAX` and returned parts outside the domain — values no `Money` constructor
/// would admit, handed back as though they were money. The `expect`s below all rest on
/// in-domain input; this is what makes that reasoning true rather than assumed.
///
/// [`AllocationError::InvalidWeights`] if `weights` is empty or every weight is zero — there
/// is no meaningful distribution, and silently returning `[]` would destroy the whole amount.
///
/// Both arms were `assert!` until an idiomatic-API review pointed out that this function had
/// two failure protocols at once: `Err` for a bad amount, panic for bad weights, in a
/// signature that already offered the caller a `Result`. Weights arrive from request bodies
/// and config files, so the panic forced every such caller to pre-validate or accept that
/// user input could abort the process.
///
/// # Panics
/// Only on a broken internal invariant: the distribution leaves a remainder smaller than the
/// number of **positive-weight** parts, and the assertion below says so. No caller input can
/// provoke it — bad input is what the `Err` arms above are for — and if it ever fired,
/// conservation would no longer be provable, which is a condition worth stopping for rather than
/// distributing around.
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
        // I256 IS REQUIRED. At the domain top this product is ~1e36 * 4.3e9 = 4.3e45,
        // which overflows i128 (max 1.7e38). Verified in DESIGN.md E11.
        let num = I256::from(units)
            .checked_mul(I256::from(i128::from(w)))
            .expect("|units * w| <= 4.3e45, ~31 orders of magnitude below I256::MAX");

        // `div_round_i256` names this function as an intended caller: its precondition is
        // `den < I256::MAX / 2`, and `total_w` is a sum of `u32`s. Going through it rather
        // than writing `/` means the truncation is a NAMED rounding mode rather than an
        // unstated property of Rust's `/`, which is the whole reason `Rounding` exists.
        // The residue it returns is dropped deliberately: it is denominated in units of
        // `total_w`, and `remainder` below re-derives the same shortfall in canonical
        // units, which is what Fowler's distribution needs. Nothing is lost by ignoring
        // it — it is an `I256`, not a canonical-unit `Residue` obligation.
        let (share, _) = div_round_i256(num, I256::from(total_w), Rounding::TowardZero);
        let share = i128::try_from(share).expect("|share| <= |units| <= DOMAIN_MAX");
        parts.push(share);

        // Every share carries the sign of `units` and the partial sum grows monotonically
        // toward it, so `remainder` only ever shrinks: |remainder| <= |units| <= DOMAIN_MAX.
        remainder = remainder
            .checked_sub(share)
            .expect("|remainder| <= |units| <= DOMAIN_MAX, ~170x below i128::MAX");
    }

    // A zero-weight share truncated to EXACTLY 0 (`units * 0 / total_w`) and so lost no
    // fraction; only positive-weight shares lost anything, each strictly less than one unit and
    // all with the same sign. So the remainder is bounded by the number of POSITIVE weights, not
    // the vector length — a tighter bound than the old `bump < weights.len()`, and the fact that
    // makes the distribution below always fit. `try_from` rather than `as`: the bound is a
    // derived invariant, not a property of the type, and an `as` cast that silently truncates
    // when a derivation turns out to be wrong is how this crate has been bitten before — in the
    // residue leak counter, whose whole job was detecting silent loss.
    let step = remainder.signum();
    let bump = usize::try_from(remainder.unsigned_abs())
        .expect("|remainder| < count of positive weights, which is a usize");
    let positive = weights.iter().filter(|&&w| w != 0).count();
    assert!(
        bump < positive,
        "allocate: {bump} leftover units for {positive} positive-weight parts — conservation is \
         no longer provable",
    );

    // The remainder goes ONLY to POSITIVE-weight positions (R2-F1). Walking positions by index
    // — `take(bump)` over all parts — handed the rounding unit to whichever slot came first,
    // including a zero-weight one, which has no claim to it: money conserved but paid to the
    // wrong party, invisible to every conservation test because the sum is unchanged. The
    // `filter` restricts the front-loading to slots that actually asked for a share.
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

        // TRUNCATING division, matching `allocate_units`, which is the reference this must not
        // diverge from. `div_euclid` was tried first and is wrong here: it floors toward
        // negative infinity, so `split(-1, 2)` returned `[0, -1]` where `allocate` returns
        // `[-1, 0]`. Both conserve and both differ by one unit, so conservation tests pass
        // either way -- only an equivalence test against the replaced expression catches it.
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
/// **O(1) state and no allocation, whatever `n` is** — the iterator holds the distribution
/// constants and a cursor, and nothing that grows with the part count. That is what makes an
/// unbounded part count a *time* cost rather than a memory one — the distinction that turns
/// `u32::MAX` from a 68.7 GB request into a long loop the caller can stop.
pub struct SplitParts<C: StaticCurrency> {
    base: i128,
    /// `+1` or `-1`: the direction the truncation remainder is handed out in, which follows the
    /// sign of the amount rather than of the divisor.
    step: i128,
    /// How many leading parts receive one extra unit.
    extra: usize,
    next: usize,
    count: usize,
    // `Money<C>` proves it uses `C` through a real field; this iterator's state is
    // currency-agnostic, so without this the parameter is unconstrained (E0392) — the same
    // structural note that applies to `Rate`.
    _currency: PhantomData<C>,
}

// `Clone` but deliberately **NOT `Copy`**, which `clippy::copy_iterator` is right about: a
// `Copy` iterator is silently duplicated by any use that moves it, so `for m in parts` would
// leave the caller's `parts` untouched at its original position and a later `parts.next()`
// would replay values already consumed. Cloning a cursor should be something a caller asks
// for. Hand-written rather than derived for the reason given at `money.rs:17`:
// `#[derive(Clone)]` would emit `impl<C: Clone>`, bounding a phantom parameter when nothing
// about the iterator's state depends on it.
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
    /// Reports progress rather than internals: the distribution constants (`base`, `step`,
    /// `extra`) are derived and say nothing a reader of a debug line wants. Hence
    /// `finish_non_exhaustive`, which prints the `..` that says so.
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
