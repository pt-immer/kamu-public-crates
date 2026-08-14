//! The typed layer: `Money<C>` arithmetic, built on the raw kernel.

use crate::Money;
use crate::StaticCurrency;
use crate::arithmetic::kernel::{add_units, div_int_units, sub_units, sum_units};
use crate::errors::AmountError;
use crate::residue::Division;
use crate::rounding::Rounding;
use core::num::NonZeroU32;

impl<C: StaticCurrency> Money<C> {
    /// Exact addition, reporting the rejected total.
    ///
    /// The same arithmetic as [`Self::checked_add`], which discards that total. Prefer this one
    /// when the refusal is reported rather than merely branched on: an operator or a log line
    /// needs the value that was refused, and recomputing it at the call site duplicates the
    /// overflow reasoning this method already carries.
    ///
    /// # Errors
    /// [`AmountError::OutOfDomain`], carrying the exact sum, if it leaves the domain.
    #[inline]
    pub const fn try_add(self, o: Self) -> Result<Self, AmountError> {
        // The shared raw kernel validates both operands and the result, so its `Some` arm
        // justifies the crate-private unchecked constructor.
        match add_units(self.units(), o.units()) {
            Some(units) => Ok(Self::from_units_unchecked(units)),
            // Both operands are `Money`, hence in domain, so the kernel refused the *result* and
            // not an operand. Two magnitudes at most `DOMAIN_MAX` sum ~85x below `i128::MAX`, so
            // this reproduces the exact total and never actually wraps.
            None => Err(AmountError::out_of_domain(self.units().wrapping_add(o.units()))),
        }
    }

    /// Exact subtraction, reporting the rejected total. See [`Self::try_add`].
    ///
    /// # Errors
    /// [`AmountError::OutOfDomain`], carrying the exact difference, if it leaves the domain.
    #[inline]
    pub const fn try_sub(self, o: Self) -> Result<Self, AmountError> {
        match sub_units(self.units(), o.units()) {
            Some(units) => Ok(Self::from_units_unchecked(units)),
            None => Err(AmountError::out_of_domain(self.units().wrapping_sub(o.units()))),
        }
    }

    /// Exact addition. `None` iff the result leaves the domain.
    ///
    /// [`Self::try_add`] is the same operation, reporting the total it refused.
    #[inline]
    #[must_use]
    pub const fn checked_add(self, o: Self) -> Option<Self> {
        // Delegating, rather than reaching for the kernel a second time, is what makes the two
        // surfaces incapable of disagreeing about which sums are money.
        match self.try_add(o) {
            Ok(sum) => Some(sum),
            Err(_) => None,
        }
    }

    /// Exact subtraction. `None` iff the result leaves the domain.
    ///
    /// [`Self::try_sub`] is the same operation, reporting the total it refused.
    #[inline]
    #[must_use]
    pub const fn checked_sub(self, o: Self) -> Option<Self> {
        match self.try_sub(o) {
            Ok(difference) => Some(difference),
            Err(_) => None,
        }
    }
}

impl<C: StaticCurrency> Money<C> {
    /// Sum any number of amounts, exactly, failing only if the **total** leaves the domain.
    ///
    /// Replaces `iter().sum()`, which would fold through panicking [`Add`](core::ops::Add) and become
    /// order-dependent when a partial sum leaves the domain. This method accumulates in
    /// `I256` and checks only the final total.
    ///
    /// The item type enforces one currency; runtime adapters must enforce that rule themselves.
    ///
    /// Accepts owned or borrowed items.
    ///
    /// # Errors
    /// [`AmountError`] if the total is outside the domain.
    ///
    /// A `Result`, and not a [`Division`](crate::Division)-style product type. Division splits
    /// one amount into two parts and bounds its residue by the divisor, so the residue is
    /// always money and can always be handed back. An overflowing sum splits nothing, creates
    /// no obligation, and bounds its excess by nothing: two terms at the domain edge overshoot
    /// by exactly the domain maximum, which is a valid [`Residue`](crate::Residue), while 171
    /// overshoot by more than `i128` holds, which is not. An `Overflow` could therefore only
    /// offer a *fallible* accessor for the excess — the `Result` again, one layer down.
    ///
    pub fn try_sum<B, I>(iter: I) -> Result<Self, AmountError>
    where
        B: core::borrow::Borrow<Self>,
        I: IntoIterator<Item = B>,
    {
        let units = sum_units(iter.into_iter().map(|m| m.borrow().units()))?;
        // `sum_units` returns an in-domain total, so this constructor cannot fail.
        Ok(Self::from_units_unchecked(units))
    }
}

impl<C: StaticCurrency> Money<C> {
    /// Divide by a positive integer, rounding per `mode`.
    ///
    /// Returns a [`Division`](crate::Division) — the quotient and the residue **bundled**, not a tuple. There
    /// is no way to reach the money without deciding what happens to the residue:
    /// [`Division::take_residue`] hands you both, [`Division::discard_deliberately`] throws
    /// the residue away by name. The identity `quotient * n + residue == self.units()` holds
    /// for every mode.
    ///
    /// The bundle is the enforcement. `let (share, _) = …` no longer compiles, because there
    /// is no tuple to destructure, and dropping an undecided `Division` is safe — nothing was
    /// handed out, so nothing left the ledger.
    ///
    /// This is **not** how you split a payment N ways: these shares will not sum back to the
    /// whole. Use [`Money::allocate`](crate::Money::allocate) for that.
    ///
    /// # Panics
    /// Panics only if the shared raw kernel rejects a typed, in-domain amount.
    pub fn div_int(self, n: NonZeroU32, mode: Rounding) -> Division<C> {
        let (q, r) = div_int_units(self.units(), n, mode)
            // Unreachable: `Money<C>` is in-domain by construction, which is the precondition
            // `div_int_units` now checks rather than documents.
            .expect("Money<C> is in-domain by construction")
            .take_residue();
        Division::new(q, r)
    }
}

#[cfg(test)]
// `q * n + residue` is the conservation oracle, computed independently of the code under test.
// The quotient is bounded by the dividend and the divisor is a `NonZeroU32`, so it is total.
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use crate::Money;
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AmountError;
    use crate::iso::USD;
    use proptest::prelude::*;

    fn m(u: i128) -> Money<USD> {
        Money::<USD>::try_from_units(u).unwrap()
    }

    #[test]
    fn checked_add_refuses_domain_overflow_loudly() {
        assert_eq!(m(DOMAIN_MAX).checked_add(m(1)), None);
        assert_eq!(m(-DOMAIN_MAX).checked_sub(m(1)), None);
    }

    #[test]
    fn try_add_and_try_sub_report_the_total_they_refused() {
        // The value, not merely the refusal: an `is_err()` assertion passes for a payload of
        // zero, which is the whole reason these methods exist beside the `checked_` pair.
        assert_eq!(m(DOMAIN_MAX).try_add(m(1)), Err(AmountError::out_of_domain(DOMAIN_MAX + 1)));
        assert_eq!(m(-DOMAIN_MAX).try_sub(m(1)), Err(AmountError::out_of_domain(-DOMAIN_MAX - 1)));
    }

    #[test]
    fn the_widest_refusable_total_is_reported_exactly_and_never_wraps() {
        // Both extremes at once, which is as far past the domain as two `Money` can reach.
        // `try_add` reconstructs this with `wrapping_add`; if that claim were wrong, it is
        // here that the payload would come back with the opposite sign. The headroom this
        // relies on is already pinned crate-wide, by the `DOMAIN_MAX + DOMAIN_MAX` assertion
        // in `domain.rs`.
        assert_eq!(m(DOMAIN_MAX).try_add(m(DOMAIN_MAX)), Err(AmountError::out_of_domain(2 * DOMAIN_MAX)));
        assert_eq!(m(-DOMAIN_MAX).try_sub(m(DOMAIN_MAX)), Err(AmountError::out_of_domain(-2 * DOMAIN_MAX)));
    }

    #[test]
    fn the_checked_and_try_surfaces_are_one_operation() {
        for a in [0, 1, -1, DOMAIN_MAX, -DOMAIN_MAX] {
            for b in [0, 1, -1, DOMAIN_MAX, -DOMAIN_MAX] {
                let (x, y) = (m(a), m(b));
                assert_eq!(x.checked_add(y), x.try_add(y).ok(), "add disagreed at {a} + {b}");
                assert_eq!(x.checked_sub(y), x.try_sub(y).ok(), "sub disagreed at {a} - {b}");
            }
        }
    }

    // `const`, not merely `#[inline]`: the pair is usable wherever `checked_add` already was,
    // and dropping that would be a silent semver break rather than a compile error here.
    const ONE: Money<USD> = match Money::<USD>::try_from_units(1) {
        Ok(amount) => amount,
        Err(_) => panic!("one canonical unit is in domain"),
    };
    const CONST_SUM: i128 = match ONE.try_add(ONE) {
        Ok(sum) => sum.units(),
        Err(_) => panic!("two canonical units are in domain"),
    };
    const _: () = assert!(CONST_SUM == 2);
    const _: () = assert!(ONE.try_sub(ONE).is_ok());

    use crate::rounding::Rounding;
    use core::num::NonZeroU32;

    #[test]
    fn div_int_returns_its_remainder() {
        // 10.000000000000000000 USD / 3 -> 3.333333333333333333 with 1 unit left over
        let (share, res) = m(10_000_000_000_000_000_000)
            .div_int(NonZeroU32::new(3).unwrap(), Rounding::TowardZero)
            .take_residue();
        assert_eq!(share.units(), 3_333_333_333_333_333_333);
        assert_eq!(res.take_units(), 1, "the lost unit is HANDED BACK, not dropped");
    }

    #[test]
    fn div_int_conserves_under_every_mode() {
        for mode in Rounding::ALL {
            let (share, res) =
                m(10_000_000_000_000_000_000).div_int(NonZeroU32::new(3).unwrap(), *mode).take_residue();
            // quotient*3 + residue == original. nothing vanishes, whatever the mode.
            assert_eq!(share.units() * 3 + res.take_units(), 10_000_000_000_000_000_000, "{mode:?}");
        }
    }

    #[test]
    fn div_int_at_the_domain_top_does_not_overflow() {
        // The I256 path must be used regardless of how small the divisor is. This pins it.
        let (share, res) =
            m(DOMAIN_MAX).div_int(NonZeroU32::new(7).unwrap(), Rounding::TowardZero).take_residue();
        assert_eq!(share.units() * 7 + res.take_units(), DOMAIN_MAX);
    }

    proptest::proptest! {
    #[test]
    fn prop_div_int_conserves(u in -DOMAIN_MAX..=DOMAIN_MAX, n in 1u32..=1000, mi in 0usize..Rounding::ALL.len()) {
        // Derive the range so new rounding modes enter the property automatically.
        use core::num::NonZeroU32;
        let mode = Rounding::ALL[mi];
        let (q, r) = Money::<USD>::try_from_units(u)
            .unwrap()
            .div_int(NonZeroU32::new(n).unwrap(), mode)
            .take_residue();
        // q*n + residue == u, exactly, for every mode and every input.
        prop_assert_eq!(q.units() * i128::from(n) + r.take_units(), u);
    }

    #[test]
    fn prop_try_sum_equals_fold(v in prop::collection::vec(-1_000_000_000i128..=1_000_000_000, 0..50)) {
        // The generated total stays within the domain.
        let ms: Vec<Money<USD>> = v.iter().map(|&u| Money::<USD>::try_from_units(u).unwrap()).collect();
        prop_assert_eq!(Money::<USD>::try_sum(&ms).unwrap().units(), v.iter().sum::<i128>());
    }
    }
}
