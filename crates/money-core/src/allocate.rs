//! Conservative distribution: splitting money without losing any.

use crate::currency::StaticCurrency;
use crate::domain::MoneyError;
use crate::money::Money;
use crate::rounding::{Rounding, div_round_i256};
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
/// [`MoneyError::DomainOverflow`] if `units` is outside the domain. Without it this function
/// accepted `i128::MAX` and returned parts outside the domain — values no `Money` constructor
/// would admit, handed back as though they were money. The `expect`s below all rest on
/// in-domain input; this is what makes that reasoning true rather than assumed.
///
/// [`MoneyError::UnallocatableWeights`] if `weights` is empty or every weight is zero — there
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
pub fn allocate_units(units: i128, weights: &[u32]) -> Result<Vec<i128>, MoneyError> {
    if !crate::domain::in_domain(units) {
        return Err(MoneyError::DomainOverflow { attempted_units: units });
    }
    let total_w: i128 = weights.iter().map(|&w| i128::from(w)).sum();
    if weights.is_empty() || total_w == 0 {
        return Err(MoneyError::UnallocatableWeights { weights: weights.len() });
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
        // it — it is an `I256`, not a `Residue`, so no drop-bomb is being defeated here.
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
    /// [`allocate`](Self::allocate), but bad weights are a **value rather than a panic**.
    ///
    /// This is the canonical typed operation for weights whose shape is not known at compile
    /// time — a request body, a config row, a file. `allocate` panics on empty or all-zero
    /// weights, and that contract is right only when the slice is a literal: `&[u32]` does not
    /// encode that it is one, and a typed `Money<C>` does not make its argument compile-time
    /// data. Panicking on ordinary runtime-invalid input is the wrong failure channel for a
    /// reusable library, and under `panic = "abort"` it costs the process.
    ///
    /// Before this existed the only fallible allocator was [`allocate_units`], which returns raw
    /// `i128` — so a service either left the typed API or repeated the conversion by hand.
    ///
    /// # Errors
    /// [`MoneyError::UnallocatableWeights`] if `weights` is empty or every weight is zero. There
    /// is no meaningful distribution in either case, and returning `[]` would destroy the amount.
    ///
    /// # Panics
    /// Never on caller input — that is the entire point of this method, and every reachable bad
    /// input is an `Err` above. The one `expect` inside is a broken internal invariant: a part
    /// larger than the whole it came from. `Money<C>` is in-domain by construction and
    /// [`allocate_units`] conserves, so `|part| <= |whole| <= DOMAIN_MAX` holds; if it ever did
    /// not, conservation would no longer be provable and stopping is the right response.
    pub fn try_allocate(self, weights: &[u32]) -> Result<Vec<Self>, MoneyError> {
        Ok(allocate_units(self.units(), weights)?
            .into_iter()
            .map(|u| Self::from_units(u).expect("|part| <= |whole| <= DOMAIN_MAX"))
            .collect())
    }

    /// Distribute across `weights`, guaranteeing `Money::try_sum(&parts) == Ok(self)`.
    ///
    /// Truncated shares leave a remainder smaller than the number of positive weights; that
    /// remainder is distributed one unit at a time from the front, **skipping zero-weight
    /// positions**, so the total is exactly preserved and a recipient with no claim receives
    /// nothing. This is Fowler's allocation.
    ///
    /// Conserves at the **canonical scale** — the only scale at which money exists here.
    ///
    /// # Panics
    /// If `weights` is empty or every weight is zero — there is no meaningful distribution,
    /// and silently returning `[]` would destroy the whole amount.
    ///
    /// Reach for [`try_allocate`](Self::try_allocate) when the weights are not a literal.
    ///
    /// Panicking is deliberate **here** and fallible in [`allocate_units`], which is not an
    /// inconsistency but the same split the crate draws everywhere else. This method is the
    /// typed path: weights are usually a literal like `&[1, 1, 1]`, so bad weights are a bug
    /// in the caller, and std panics on the same shape (`chunks(0)`, `split_at` past the end).
    /// [`allocate_units`] is the runtime path an adapter reaches for, where the weights came
    /// off a request body and refusing them is an ordinary outcome rather than a defect.
    #[must_use]
    pub fn allocate(self, weights: &[u32]) -> Vec<Self> {
        allocate_units(self.units(), weights)
            // `Money<C>` is in-domain by construction, so the only reachable error is the
            // weights one — which this method's contract says is a panic.
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .map(|u| Self::from_units(u).expect("|part| <= |whole| <= DOMAIN_MAX"))
            .collect()
    }

    /// Split into `n` as-equal-as-possible parts that sum to exactly `self`.
    ///
    /// Equal weights are computed directly rather than materialised. The previous version
    /// built `vec![1u32; n]` purely to hand it to [`Self::allocate`], so an `n` the caller
    /// chose sized a `u32` allocation, an `i128` allocation, and the result — three vectors
    /// alive at once for weights that are all the same number. `n` is a `NonZeroU32`, so a
    /// caller passing `u32::MAX` reserved ~17GB before this changed.
    ///
    /// Conserves exactly, by the same rule [`Self::allocate`] uses: truncated shares leave a
    /// remainder below `n`, distributed one unit at a time from the front.
    ///
    /// # Cost, stated rather than left to be discovered
    ///
    /// **O(n) time and O(n) memory.** The returned `Vec` holds exactly `n` `Money<C>` values at
    /// 16 bytes each, and `collect` reserves all of them before the first one is written.
    /// `NonZeroU32` admits `u32::MAX`, so a part count taken from a request asks for roughly
    /// 68.7 GB on a 64-bit target — the value passes every validation this crate performs,
    /// because "how many ways may this be split" is a business question and the answer is not
    /// the same in two applications.
    ///
    /// This crate therefore does **not** invent a cap; it offers ways not to need one. For a
    /// count taken from untrusted input that way is [`split_iter`](Self::split_iter), which
    /// allocates nothing at all. [`try_split`](Self::try_split) returns the allocator's refusal
    /// where there is one, but its guarantee is narrower than its name suggests and its own
    /// documentation says where it stops. A service taking `n` from
    /// untrusted input should still bound it at its own boundary — `kamu-money-pg` does exactly
    /// that, capping SQL-side output at `MAX_ALLOCATE_PARTS`, which is policy and lives with the
    /// consumer rather than here.
    ///
    /// # Panics
    /// If `n` exceeds `usize::MAX` on this target. A silently truncated part count would
    /// split the money a different number of ways than the caller asked for.
    ///
    /// **Allocation failure aborts** rather than unwinding, because that is what the global
    /// allocator does on OOM and `Vec` gives this function no way to report it. That is not a
    /// choice made here, and it is the entire reason [`try_split`](Self::try_split) exists.
    #[must_use]
    pub fn split(self, n: NonZeroU32) -> Vec<Self> {
        self.split_iter(n).collect()
    }

    /// [`split`](Self::split), but the **allocator's refusal** is returned instead of aborting.
    ///
    /// Reserves the exact capacity first, so a request the allocator declines is refused before
    /// any part is computed. `Vec::with_capacity` and `collect` abort instead of returning, so
    /// `try_reserve_exact` is the only way to ask the question at all.
    ///
    /// # This does NOT make a large `n` safe, and the difference matters
    ///
    /// `try_reserve_exact` reports exactly two things: a capacity that overflows, and an
    /// allocator that declines. **It cannot promise the memory stays available while the vector
    /// is filled.** On a Linux host with overcommit — the default — a 68.7 GB reservation can
    /// SUCCEED, handing back address space that is not backed by anything, and the process is
    /// then killed as `extend` touches the pages. No `Err` is returned on that path, because no
    /// Rust allocator API observes it.
    ///
    /// So this is **not** the safe path for a request-derived count. [`split_iter`](Self::split_iter)
    /// is, because it never asks for the memory. What `try_split` buys is a clean error where the
    /// allocator genuinely refuses — a 32-bit target, a cgroup that fails the mapping, a capacity
    /// that overflows — instead of an abort.
    ///
    /// Collecting `n` parts safely needs a bound the **caller** enforces. This crate does not
    /// invent one, for the reason given on [`split`](Self::split).
    ///
    /// # Errors
    /// [`TryReserveError`] if the capacity overflows or the allocator refuses the reservation.
    ///
    /// # Panics
    /// If `n` exceeds `usize::MAX` on this target — the same condition as [`split`](Self::split)
    /// and deliberately still a panic. A part count that does not fit the address space is a
    /// caller bug, not a memory shortage, and reporting it as one would tell the caller to retry
    /// something that can never succeed.
    pub fn try_split(self, n: NonZeroU32) -> Result<Vec<Self>, TryReserveError> {
        let parts = self.split_iter(n);
        let mut out = Vec::new();
        out.try_reserve_exact(parts.len())?;
        out.extend(parts);
        Ok(out)
    }

    /// The parts of a split, computed one at a time and **allocating nothing**.
    ///
    /// Every part is a pure function of its index, so there is no state to accumulate and no
    /// reason the whole distribution has to exist at once. A caller summing, streaming or
    /// writing out the parts never materialises them; a caller that genuinely wants a `Vec`
    /// calls [`split`](Self::split) or [`try_split`](Self::try_split), both of which are this
    /// iterator plus a collection strategy.
    ///
    /// The iterator is [`ExactSizeIterator`], which is what lets `try_split` reserve exactly
    /// once, and yields parts in the same order [`allocate`](Self::allocate) does.
    ///
    /// # Panics
    /// If `n` exceeds `usize::MAX` on this target — see [`split`](Self::split).
    #[must_use]
    pub fn split_iter(self, n: NonZeroU32) -> SplitParts<C> {
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

/// The lazy half of [`Money::split`], returned by [`Money::split_iter`].
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
        Some(Money::from_units(units).expect("|part| <= |whole| <= DOMAIN_MAX"))
    }

    /// Exact, which is what lets [`Money::try_split`] reserve once and correctly.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.count.saturating_sub(self.next);
        (remaining, Some(remaining))
    }
}

impl<C: StaticCurrency> ExactSizeIterator for SplitParts<C> {}
impl<C: StaticCurrency> core::iter::FusedIterator for SplitParts<C> {}
