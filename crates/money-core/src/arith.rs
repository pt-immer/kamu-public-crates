//! Exact arithmetic. Rounding is not merely discouraged here — it is unrepresentable.

use crate::Money;
use crate::StaticCurrency;
use crate::error_impl::AmountError;
use crate::residue_impl::{Division, UntaggedDivision};
use crate::rounding_impl::{Rounding, div_round_i256};
use core::num::NonZeroU32;
use core::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use ethnum::I256;

impl<C: StaticCurrency> Money<C> {
    /// Exact addition. `None` iff the result leaves the domain.
    #[inline]
    #[must_use]
    pub const fn checked_add(self, o: Self) -> Option<Self> {
        // The shared raw kernel validates both operands and the result, so its `Some` arm
        // justifies the crate-private unchecked constructor.
        match add_units(self.units(), o.units()) {
            Some(units) => Some(Self::from_units_unchecked(units)),
            None => None,
        }
    }

    /// Exact subtraction. `None` iff the result leaves the domain.
    #[inline]
    #[must_use]
    pub const fn checked_sub(self, o: Self) -> Option<Self> {
        match sub_units(self.units(), o.units()) {
            Some(units) => Some(Self::from_units_unchecked(units)),
            None => None,
        }
    }
}

/// Panics on domain overflow, matching integer `Add`. Use [`Money::checked_add`] when overflow
/// is recoverable.
impl<C: StaticCurrency> Add for Money<C> {
    type Output = Self;
    #[inline]
    fn add(self, o: Self) -> Self {
        self.checked_add(o).unwrap_or_else(|| {
            let attempted =
                self.units().checked_add(o.units()).expect("two in-domain amounts cannot overflow i128");
            panic!("{}", AmountError::out_of_domain(attempted))
        })
    }
}

impl<C: StaticCurrency> Sub for Money<C> {
    type Output = Self;
    #[inline]
    fn sub(self, o: Self) -> Self {
        self.checked_sub(o).unwrap_or_else(|| {
            let attempted =
                self.units().checked_sub(o.units()).expect("two in-domain amounts cannot overflow i128");
            panic!("{}", AmountError::out_of_domain(attempted))
        })
    }
}

impl<C: StaticCurrency> Neg for Money<C> {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        // The domain excludes `i128::MIN`, so negation is total for every valid `Money`.
        Self::from_units_unchecked(self.units().wrapping_neg())
    }
}

impl<C: StaticCurrency> Money<C> {
    /// Sum any number of amounts, exactly, failing only if the **total** leaves the domain.
    ///
    /// Replaces `iter().sum()`, which would fold through panicking [`Add`] and become
    /// order-dependent when a partial sum leaves the domain. This method accumulates in
    /// [`I256`] and checks only the final total.
    ///
    /// The item type enforces one currency; runtime adapters must enforce that rule themselves.
    ///
    /// Accepts owned or borrowed items.
    ///
    /// # Errors
    /// [`AmountError`] if the total is outside the domain.
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

/// Add two canonical unit counts. `None` if **either operand** or the result is outside the
/// domain.
///
/// This is the shared `Money`/`kamu-money-pg` addition kernel: both
/// [`Money::checked_add`] and `kamu-money-pg`'s generated `<type>_add` functions delegate here.
#[inline]
#[must_use]
pub const fn add_units(a: i128, b: i128) -> Option<i128> {
    // Raw callers do not carry Money's invariant. Check operands as well as the result so
    // out-of-domain values cannot cancel into a valid-looking amount.
    if !crate::domain_impl::in_domain(a) || !crate::domain_impl::in_domain(b) {
        return None;
    }
    match a.checked_add(b) {
        Some(v) => {
            if crate::domain_impl::in_domain(v) {
                Some(v)
            } else {
                None
            }
        }
        None => None,
    }
}

/// Subtract two canonical unit counts. `None` if **either operand** or the result is outside
/// the domain. The units-level kernel behind both [`Money::checked_sub`] and `kamu-money-pg`'s
/// the generated `<type>_sub` functions.
#[inline]
#[must_use]
pub const fn sub_units(a: i128, b: i128) -> Option<i128> {
    // Operands enforced, not assumed — see `add_units`. Without this,
    // `sub_units(i128::MAX, i128::MAX)` returns `Some(0)`.
    if !crate::domain_impl::in_domain(a) || !crate::domain_impl::in_domain(b) {
        return None;
    }
    match a.checked_sub(b) {
        Some(v) => {
            if crate::domain_impl::in_domain(v) {
                Some(v)
            } else {
                None
            }
        }
        None => None,
    }
}

/// Sum canonical units exactly, checking the domain **once**, at the end.
///
/// Shared by [`Money::try_sum`] and `kamu-money-pg`'s `sum()` aggregates.
///
/// Accumulating in [`I256`] and narrowing once avoids an `i128` fold whose transient partial
/// sum leaves the domain before the final total returns to it. That behavior would make
/// `Sum` order-dependent.
///
/// # Errors
/// [`AmountError::OutOfDomain`] if any term or exact `i128` total leaves the domain;
/// [`AmountError::ArithmeticOverflow`] if the wide total cannot be represented as `i128`.
///
pub fn sum_units<I: IntoIterator<Item = i128>>(units: I) -> Result<i128, AmountError> {
    let mut acc = UnitSum::ZERO;
    for u in units {
        acc = acc.add_units(u)?;
    }
    acc.finish()
}

/// Wide accumulator behind [`sum_units`] and `kamu-money-pg`'s per-currency `sum()` aggregates.
///
/// * every **term** is domain-checked as it enters ([`Self::add_units`]) — a total computed from
///   a term that was never money is not a total;
/// * accumulation is [`I256`], so a partial sum may leave the domain and come back;
/// * the domain is checked **once**, on the way out ([`Self::finish`]);
/// * [`Self::merge`] is associative and commutative, keeping parallel plans deterministic.
///
/// # Encoding
///
/// [`Self::to_le_bytes`] and [`Self::from_le_bytes`] provide an explicit little-endian encoding
/// for transfer between parallel workers.
///
/// [`Self::from_le_bytes`] accepts **any** 32 bytes, so a decoded accumulator can hold a value no
/// sequence of in-domain terms could produce. That is why [`Self::add_units`] and [`Self::merge`]
/// return `Result` instead of asserting: a forged or corrupt state must be an error, not a panic
/// inside a database backend.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitSum(I256);

impl UnitSum {
    /// The empty sum.
    ///
    /// Deliberately not a `Money`: it carries no currency. An empty SQL aggregate returns NULL
    /// for exactly that reason — there is no currency-free zero in this design.
    pub const ZERO: Self = Self(I256::ZERO);

    /// Width of the [`Self::to_le_bytes`] encoding.
    pub const ENCODED_BYTES: usize = 32;

    /// Add one term, enforcing the domain on **that term**.
    ///
    /// # Errors
    /// [`AmountError`] naming the offending term. Enforced per term, not assumed
    /// of the caller: checking only the total lets terms that were never money cancel into a
    /// plausible one — `[i128::MAX, -i128::MAX]` would otherwise sum to `Ok(0)`.
    ///
    /// Also if the accumulator itself leaves `I256`. Unreachable for a state this type built —
    /// each term is below 1e36 and `I256::MAX` is ~5.7e76, so that needs ~5.7e40 in-domain terms
    /// — but reachable for one that arrived through [`Self::from_le_bytes`].
    pub fn add_units(self, units: i128) -> Result<Self, AmountError> {
        if !crate::domain_impl::in_domain(units) {
            return Err(AmountError::out_of_domain(units));
        }
        match self.0.checked_add(I256::from(units)) {
            Some(acc) => Ok(Self(acc)),
            None => Err(AmountError::ArithmeticOverflow),
        }
    }

    /// Combine two partial sums.
    ///
    /// Associative and commutative, which is precisely what lets PostgreSQL combine parallel
    /// partials in whatever order its workers happen to finish and still produce one answer.
    ///
    /// # Errors
    /// [`AmountError`] if the combined accumulator leaves `I256`. Unreachable for
    /// states this type built; reachable for one decoded from arbitrary bytes.
    pub fn merge(self, other: Self) -> Result<Self, AmountError> {
        match self.0.checked_add(other.0) {
            Some(acc) => Ok(Self(acc)),
            None => Err(AmountError::ArithmeticOverflow),
        }
    }

    /// Narrow once, and check the domain once.
    ///
    /// # Errors
    /// [`AmountError::OutOfDomain`] when an exact `i128` total leaves the domain, or
    /// [`AmountError::ArithmeticOverflow`] when the total cannot be represented as `i128`.
    pub fn finish(self) -> Result<i128, AmountError> {
        let attempted = i128::try_from(self.0).map_err(|_| AmountError::ArithmeticOverflow)?;
        if crate::domain_impl::in_domain(attempted) {
            Ok(attempted)
        } else {
            Err(AmountError::out_of_domain(attempted))
        }
    }

    /// The accumulator as 32 little-endian bytes, in the byte order documented on the type.
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; Self::ENCODED_BYTES] {
        self.0.to_le_bytes()
    }

    /// Rebuild an accumulator from [`Self::to_le_bytes`].
    ///
    /// Total: every 32-byte string is a representable accumulator, so this cannot fail. What it
    /// cannot promise is that the value is *reachable* — see the type's note on why the
    /// arithmetic methods return `Result`.
    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; Self::ENCODED_BYTES]) -> Self {
        Self(I256::from_le_bytes(bytes))
    }
}

impl<C: StaticCurrency> AddAssign for Money<C> {
    #[inline]
    fn add_assign(&mut self, o: Self) {
        *self = Add::add(*self, o);
    }
}

impl<C: StaticCurrency> SubAssign for Money<C> {
    #[inline]
    fn sub_assign(&mut self, o: Self) {
        *self = Sub::sub(*self, o);
    }
}

impl<C: StaticCurrency> Add<&Self> for Money<C> {
    type Output = Self;
    #[inline]
    fn add(self, o: &Self) -> Self {
        Add::add(self, *o)
    }
}

impl<C: StaticCurrency> Sub<&Self> for Money<C> {
    type Output = Self;
    #[inline]
    fn sub(self, o: &Self) -> Self {
        Sub::sub(self, *o)
    }
}

impl<C: StaticCurrency> Neg for &Money<C> {
    type Output = Money<C>;
    #[inline]
    fn neg(self) -> Money<C> {
        Neg::neg(*self)
    }
}

impl<C: StaticCurrency> Money<C> {
    /// Divide by a positive integer, rounding per `mode`.
    ///
    /// Returns a [`Division`] — the quotient and the residue **bundled**, not a tuple. There
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
    /// whole. Use [`Money::allocate`] for that.
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

/// The non-generic core of [`Money::div_int`], in canonical units.
///
/// For adapters that learn the currency at run time; see [`UntaggedDivision`] for why this is
/// not the API a Rust program should reach for.
///
/// # Errors
/// [`AmountError`] if `units` is outside the domain.
///
/// This arm replaces a doc comment that stated the precondition and left it unenforced —
/// "never, **for any in-domain** `units`" was a caller obligation nothing checked, and the
/// function accepted `i128::MAX` and returned a quotient outside the domain.
///
/// # Panics
/// Panics only if the rounding kernel returns a quotient larger than the dividend or a
/// residue at least as large as the divisor.
pub fn div_int_units(units: i128, n: NonZeroU32, mode: Rounding) -> Result<UntaggedDivision, AmountError> {
    if !crate::domain_impl::in_domain(units) {
        return Err(AmountError::out_of_domain(units));
    }
    let (q, r) = div_round_i256(I256::from(units), I256::from(i128::from(n.get())), mode);
    // |q| <= |units| <= DOMAIN_MAX and |r| < n, so both conversions are total.
    let q = i128::try_from(q).expect("quotient magnitude cannot exceed the dividend");
    let r = i128::try_from(r).expect("residue magnitude is below the divisor");
    Ok(UntaggedDivision::new(q, r))
}

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::arith_impl::{UnitSum, sum_units};
    use crate::domain_impl::DOMAIN_MAX;
    use crate::error_impl::AmountError;
    use crate::iso::USD;

    fn m(u: i128) -> Money<USD> {
        Money::<USD>::try_from_units(u).unwrap()
    }

    #[test]
    fn add_and_sub_are_exact() {
        assert_eq!((m(10_500_000_000_000) + m(2_250_000_000_000)).units(), 12_750_000_000_000);
        assert_eq!((m(1) - m(2)).units(), -1);
        assert_eq!((-m(5)).units(), -5);
    }

    #[test]
    fn checked_add_refuses_domain_overflow_loudly() {
        assert_eq!(m(DOMAIN_MAX).checked_add(m(1)), None);
        assert_eq!(m(-DOMAIN_MAX).checked_sub(m(1)), None);
    }

    #[test]
    fn add_units_and_sub_units_are_the_shared_kernel() {
        use super::{add_units, sub_units};
        assert_eq!(add_units(100, 200), Some(300));
        assert_eq!(sub_units(300, 200), Some(100));
        assert_eq!(add_units(DOMAIN_MAX, 0), Some(DOMAIN_MAX));
        assert_eq!(add_units(DOMAIN_MAX, 1), None, "one past the top leaves the domain");
        assert_eq!(sub_units(-DOMAIN_MAX, 1), None);
        assert_eq!(add_units(DOMAIN_MAX, -DOMAIN_MAX), Some(0));
    }

    /// These are PUBLIC functions over raw `i128`, so the domain is a precondition they must
    /// ENFORCE rather than assume — the typed wrapper's invariant does not reach them. The
    /// dangerous shape is cancellation: two operands that were never money summing to a
    /// perfectly ordinary answer. Checking only the result returns `Some(0)` here and hands a
    /// caller a value laundered out of corrupt input.
    #[test]
    fn the_raw_kernels_refuse_out_of_domain_operands_even_when_they_cancel() {
        use super::{add_units, sub_units, sum_units};

        // The cancellation cases: the RESULT is impeccable, the operands are not.
        assert_eq!(add_units(i128::MAX, -i128::MAX), None, "cancels to 0");
        assert_eq!(sub_units(i128::MAX, i128::MAX), None, "cancels to 0");
        assert_eq!(
            sum_units([i128::MAX, -i128::MAX]),
            Err(AmountError::out_of_domain(i128::MAX)),
            "the offending TERM names itself, not the total"
        );

        // The sharpest case for the OPERAND guard specifically: a bad operand whose RESULT is
        // impeccably in-domain. Without the guard this returns Some(DOMAIN_MAX) — a valid-looking
        // amount computed from an input that was never money. The `past, 0` style cases below
        // leave the domain in the result too, so they would fail with or without the guard.
        assert_eq!(add_units(DOMAIN_MAX + 1, -1), None, "result would be in-domain");
        assert_eq!(sub_units(DOMAIN_MAX + 1, 1), None, "result would be in-domain");

        // One past the top, on either side, in either operand position.
        let past = DOMAIN_MAX + 1;
        assert_eq!(add_units(past, 0), None);
        assert_eq!(add_units(0, past), None);
        assert_eq!(sub_units(-past, 0), None);
        assert_eq!(sub_units(0, -past), None);
        assert!(sum_units([1, past, 1]).is_err());

        // The domain edges themselves still pass: this refuses invalid input, not valid money.
        assert_eq!(add_units(DOMAIN_MAX, -DOMAIN_MAX), Some(0));
        assert_eq!(sum_units([DOMAIN_MAX, -DOMAIN_MAX]), Ok(0));
    }

    #[test]
    #[should_panic(expected = "outside the supported range")]
    fn plus_panics_on_domain_overflow() {
        let _ = m(DOMAIN_MAX) + m(1);
    }

    #[test]
    fn panic_message_names_the_attempted_value() {
        let e = std::panic::catch_unwind(|| m(DOMAIN_MAX) + m(1)).unwrap_err();
        let msg = e.downcast_ref::<String>().map_or("", String::as_str);
        assert!(msg.contains(&(DOMAIN_MAX + 1).to_string()), "must report what was attempted: {msg}");
    }

    #[test]
    fn try_sum_is_order_independent_at_the_domain_edge() {
        // A bounded Add fold can fail on MAX+MAX even though the exact total is
        // in-domain. try_sum accumulates wide and checks once.
        let max = m(DOMAIN_MAX);
        let neg = m(-DOMAIN_MAX);
        for order in [[max, max, neg], [max, neg, max], [neg, max, max]] {
            assert_eq!(
                Money::<USD>::try_sum(order).unwrap().units(),
                DOMAIN_MAX,
                "every order of one multiset must give the same in-domain total"
            );
        }
    }

    #[test]
    fn try_sum_reports_a_total_that_truly_leaves_the_domain() {
        assert!(Money::<USD>::try_sum([m(DOMAIN_MAX), m(DOMAIN_MAX)]).is_err());
        assert!(Money::<USD>::try_sum([m(-DOMAIN_MAX), m(-DOMAIN_MAX)]).is_err());
    }

    #[test]
    fn try_sum_of_nothing_is_zero() {
        assert_eq!(Money::<USD>::try_sum(core::iter::empty::<Money<USD>>()).unwrap().units(), 0);
    }

    #[test]
    fn try_sum_takes_values_or_references() {
        let v = vec![m(1), m(2), m(3)];
        assert_eq!(Money::<USD>::try_sum(v.iter()).unwrap().units(), 6);
        assert_eq!(Money::<USD>::try_sum(&v).unwrap().units(), 6);
        assert_eq!(Money::<USD>::try_sum(v).unwrap().units(), 6);
    }

    // --- UnitSum: the rule a SQL aggregate has to obey one row at a time -----------------------

    /// `I256::MAX`. Little-endian, so index 31 is the most significant byte.
    fn forged_max_state() -> UnitSum {
        let mut bytes = [0xFF_u8; UnitSum::ENCODED_BYTES];
        bytes[31] = 0x7F;
        UnitSum::from_le_bytes(bytes)
    }

    #[test]
    fn unit_sum_agrees_with_sum_units_however_the_terms_are_partitioned() {
        // Every partition must match the flat sum. Two partial sums leave the domain; the final
        // total does not.
        let terms = [DOMAIN_MAX, DOMAIN_MAX, -DOMAIN_MAX, -7, 3, 0];
        let flat = sum_units(terms).expect("the multiset totals in-domain");

        for split in 0..=terms.len() {
            let (left, right) = terms.split_at(split);
            let fold = |acc: UnitSum, xs: &[i128]| {
                xs.iter().try_fold(acc, |a, &u| a.add_units(u)).expect("every term is in domain")
            };
            let merged = fold(UnitSum::ZERO, left)
                .merge(fold(UnitSum::ZERO, right))
                .expect("two in-domain partials cannot overflow I256")
                .finish()
                .expect("the total is in domain");
            assert_eq!(merged, flat, "partition at {split} disagreed with the flat sum");
        }
    }

    #[test]
    fn unit_sum_merge_is_commutative_across_a_domain_edge() {
        // `[MAX, MAX]` is a partial that has LEFT the domain; `[-MAX]` brings it back. A narrow
        // state failed here, and failed asymmetrically depending on which side arrived first.
        let a = UnitSum::ZERO
            .add_units(DOMAIN_MAX)
            .and_then(|s| s.add_units(DOMAIN_MAX))
            .expect("a partial may leave the domain");
        let b = UnitSum::ZERO.add_units(-DOMAIN_MAX).expect("in domain");
        assert_eq!(a.merge(b).unwrap().finish().unwrap(), b.merge(a).unwrap().finish().unwrap());
        assert_eq!(a.merge(b).unwrap().finish().unwrap(), DOMAIN_MAX);
    }

    #[test]
    fn unit_sum_enforces_the_domain_per_term_not_only_on_the_total() {
        // Without this, `[i128::MAX, -i128::MAX]` totals to a plausible `Ok(0)` out of two terms
        // that were never money.
        let e = UnitSum::ZERO.add_units(i128::MAX).unwrap_err();
        assert_eq!(e, AmountError::out_of_domain(i128::MAX));
    }

    #[test]
    fn unit_sum_survives_the_byte_round_trip_the_aggregate_needs() {
        // A transition state crosses a process boundary between parallel workers. If it did not
        // decode to itself, a parallel plan would silently total something else.
        for units in [0, 1, -1, DOMAIN_MAX, -DOMAIN_MAX] {
            let s = UnitSum::ZERO.add_units(units).expect("in domain");
            assert_eq!(UnitSum::from_le_bytes(s.to_le_bytes()), s);
            assert_eq!(UnitSum::from_le_bytes(s.to_le_bytes()).finish().unwrap(), units);
        }
    }

    #[test]
    fn a_forged_state_is_an_error_never_a_panic() {
        // `from_le_bytes` accepts any 32 bytes, so a corrupt or hand-written SQL transition state
        // can hold a value no sequence of in-domain terms could reach. Inside a database backend
        // that has to be an error: a panic there is an ereport at best and an abort at worst.
        let huge = forged_max_state();
        assert_eq!(huge.add_units(1), Err(AmountError::ArithmeticOverflow));
        assert_eq!(huge.merge(huge), Err(AmountError::ArithmeticOverflow));
        assert_eq!(huge.finish(), Err(AmountError::ArithmeticOverflow));
    }

    use crate::rounding_impl::Rounding;
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
}
