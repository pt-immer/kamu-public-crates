//! Exact arithmetic. Rounding is not merely discouraged here — it is unrepresentable.

use crate::currency::StaticCurrency;
use crate::domain::MoneyError;
use crate::money::Money;
use crate::residue::{Division, UntaggedDivision};
use crate::rounding::{Rounding, div_round_i256};
use core::num::NonZeroU32;
use core::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use ethnum::I256;

impl<C: StaticCurrency> Money<C> {
    /// Exact addition. `None` iff the result leaves the domain.
    #[inline]
    #[must_use]
    pub const fn checked_add(self, o: Self) -> Option<Self> {
        // Delegates to the non-generic `add_units` kernel `kamu-money-pg`'s `kmoney_add` also uses, so
        // the Rust and SQL layers cannot disagree about addition. `from_units` re-checks the
        // domain (cheap, always `Some` here) rather than introduce an unchecked constructor.
        match add_units(self.units(), o.units()) {
            Some(v) => Self::from_units(v),
            None => None,
        }
    }

    /// Exact subtraction. `None` iff the result leaves the domain.
    #[inline]
    #[must_use]
    pub const fn checked_sub(self, o: Self) -> Option<Self> {
        match sub_units(self.units(), o.units()) {
            Some(v) => Self::from_units(v),
            None => None,
        }
    }

    /// Exact negation. `None` iff the result leaves the domain.
    ///
    /// `i128::MIN.checked_neg()` is `None` (two's-complement asymmetry), but `i128::MIN` is
    /// already outside the domain, so this only ever returns `None` for a corrupted value.
    #[inline]
    #[must_use]
    pub const fn checked_neg(self) -> Option<Self> {
        match self.units().checked_neg() {
            Some(v) => Self::from_units(v),
            None => None,
        }
    }
}

/// Panics on domain overflow, matching std's convention for `+` on integers.
///
/// Domain overflow means ~1e18 currency units — for IDR, roughly 111 times Indonesia's
/// entire M2 money supply, and ~$62.5 trillion. It is a bug, not a condition. Use [`Money::checked_add`] where
/// you genuinely need to handle it.
impl<C: StaticCurrency> Add for Money<C> {
    type Output = Self;
    #[inline]
    fn add(self, o: Self) -> Self {
        self.checked_add(o).unwrap_or_else(|| {
            // `wrapping_add` cannot actually wrap here: both operands are in-domain, so the
            // sum is at most 2e36 — ~85x below i128::MAX. It is used only to satisfy
            // clippy::arithmetic_side_effects while reporting the TRUE attempted value.
            panic!("{}", MoneyError::DomainOverflow { attempted_units: self.units().wrapping_add(o.units()) })
        })
    }
}

impl<C: StaticCurrency> Sub for Money<C> {
    type Output = Self;
    #[inline]
    fn sub(self, o: Self) -> Self {
        self.checked_sub(o).unwrap_or_else(|| {
            panic!("{}", MoneyError::DomainOverflow { attempted_units: self.units().wrapping_sub(o.units()) })
        })
    }
}

impl<C: StaticCurrency> Neg for Money<C> {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        self.checked_neg().unwrap_or_else(|| {
            panic!("{}", MoneyError::DomainOverflow { attempted_units: self.units().wrapping_neg() })
        })
    }
}

impl<C: StaticCurrency> Money<C> {
    /// Sum any number of amounts, exactly, failing only if the **total** leaves the domain.
    ///
    /// The variadic replacement for `iter().sum()`, which this crate deliberately does **not**
    /// implement. `Sum` can only fold through the panicking [`Add`], so a partial sum can leave
    /// the domain while the total stays inside it: `[MAX, MAX, -MAX]` panics on the transient
    /// `2 * MAX`, while `[MAX, -MAX, MAX]` returns `MAX`. Same multiset, different traversal,
    /// different outcome — `.sum()` was order-dependent, and in PostgreSQL plan-dependent
    /// (R2-F4). Accumulating in [`I256`] and narrowing once removes the transient entirely: the
    /// result is a function of the values, not their order, and the one way it fails is a
    /// genuinely out-of-domain total.
    ///
    /// Cross-currency summing is not expressible here at all — every item is `Money<C>` for one
    /// `C`, so there is nothing to check at run time. That is the difference from SQL's
    /// `kmoney_sum`, whose operands only reveal their currency at run time and so must be
    /// compared there.
    ///
    /// Accepts owned or borrowed items — `try_sum(v)`, `try_sum(&v)`, `try_sum(v.iter())` — so
    /// it drops in wherever `iter().sum()` stood without a `.copied()` dance.
    ///
    /// # Errors
    /// [`MoneyError::DomainOverflow`] if the total is outside the domain.
    ///
    /// # Panics
    /// Never, for any realistic input. The single internal `expect` guards the `I256`
    /// accumulator, which would need ~5.7e40 domain-max terms to overflow — ~10^30 times more
    /// values than any machine can hold. An out-of-domain *total* is an `Err`, not a panic.
    pub fn try_sum<B, I>(iter: I) -> Result<Self, MoneyError>
    where
        B: core::borrow::Borrow<Self>,
        I: IntoIterator<Item = B>,
    {
        let units = sum_units(iter.into_iter().map(|m| m.borrow().units()))?;
        // `sum_units` returns an in-domain total, so this constructor cannot fail.
        Ok(Self::from_units(units).expect("sum_units guarantees an in-domain total"))
    }
}

/// Add two canonical unit counts. `None` if **either operand** or the result is outside the
/// domain (or, unreachably for two in-domain inputs, overflows `i128` — their sum is at most
/// ~2e36, ~85x below `i128::MAX`).
///
/// This is the ONE definition of `Money`/`kmoney` addition at the units level: both
/// [`Money::checked_add`] and `kamu-money-pg`'s `kmoney_add` delegate here, so a change to the
/// semantics reaches the Rust and the SQL surface together (C9 / specs.md §0.1).
#[inline]
#[must_use]
pub const fn add_units(a: i128, b: i128) -> Option<i128> {
    // The precondition is ENFORCED here, not assumed. This is a PUBLIC function over raw
    // `i128`, so the typed wrapper's invariant does not reach it: checking only the result
    // lets two out-of-domain operands CANCEL into a valid-looking answer —
    // `add_units(i128::MAX, -i128::MAX)` would return `Some(0)`, laundering corrupt input
    // into money. The same rule `allocate_units` and `div_int_units` already follow.
    if !crate::domain::in_domain(a) || !crate::domain::in_domain(b) {
        return None;
    }
    match a.checked_add(b) {
        Some(v) => {
            if crate::domain::in_domain(v) {
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
/// `kmoney_sub`.
#[inline]
#[must_use]
pub const fn sub_units(a: i128, b: i128) -> Option<i128> {
    // Operands enforced, not assumed — see `add_units`. Without this,
    // `sub_units(i128::MAX, i128::MAX)` returns `Some(0)`.
    if !crate::domain::in_domain(a) || !crate::domain::in_domain(b) {
        return None;
    }
    match a.checked_sub(b) {
        Some(v) => {
            if crate::domain::in_domain(v) {
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
/// The non-generic core of [`Money::try_sum`], and the same kernel `kamu-money-pg`'s `kmoney_sum`
/// calls — so the wide-accumulate-then-narrow rule lives in one place and the two layers cannot
/// disagree about what a sum of money is. This is C9's principle applied to addition, the same
/// way [`allocate_units`](crate::allocate::allocate_units) shares one distribution.
///
/// Accumulating in [`I256`] and narrowing once is the whole point: a fold through `i128` `+`
/// can leave the domain on a transient partial sum that the true total returns to, which is
/// what made `Sum` order-dependent (R2-F4). This is a function of the values, not their order.
///
/// # Errors
/// [`MoneyError::DomainOverflow`] if **any term** is outside the domain, or if the total is.
/// A bad term names itself in `attempted_units`, so a caller is told which input was not money
/// rather than handed a plausible total computed from one that never was.
///
/// # Panics
/// Never, for any realistic input — the internal `expect` guards an `I256` accumulator that
/// would need ~5.7e40 domain-max terms to overflow.
pub fn sum_units<I: IntoIterator<Item = i128>>(units: I) -> Result<i128, MoneyError> {
    let mut acc = UnitSum::ZERO;
    for u in units {
        acc = acc.add_units(u)?;
    }
    acc.finish()
}

/// Narrow an accumulator for REPORTING, saturating rather than wrapping.
///
/// A total that fits `i128` but leaves the domain (e.g. `2 * MAX = 2e36`) reports its true value;
/// a total too large even for `i128` (~170 domain-max terms) saturates to the `i128` bound of its
/// sign — still read as "far past the domain", never a wrong exact number. `try_from`, never
/// `as`: a silent truncation here would turn an impossible total into a plausible one.
fn narrow_saturating(acc: I256) -> i128 {
    i128::try_from(acc).unwrap_or(if acc > I256::ZERO { i128::MAX } else { i128::MIN })
}

/// The wide accumulator behind [`sum_units`] and PostgreSQL's `sum(kmoney)` aggregate.
///
/// [`sum_units`] receives every term at once. **A SQL aggregate cannot.** PostgreSQL hands its
/// transition function one row at a time, and — when the plan is parallel — hands its combine
/// function two partial states built by different workers. Both need the rule [`sum_units`]
/// applies, or the two layers disagree about what a sum of money is, which is the disagreement
/// R2-F4 was about in the first place.
///
/// So the rule is stated once, as a value:
///
/// * every **term** is domain-checked as it enters ([`Self::add_units`]) — a total computed from
///   a term that was never money is not a total;
/// * accumulation is [`I256`], so a partial sum may leave the domain and come back;
/// * the domain is checked **once**, on the way out ([`Self::finish`]);
/// * [`Self::merge`] is associative and commutative, so the answer belongs to the multiset rather
///   than to the plan.
///
/// The `sum(kmoney)` aggregate removed by R2-F4 failed the last two: its transition state was a
/// `kmoney`, so it re-checked the domain on every partial, and `[MAX, MAX, -MAX]` then succeeded
/// or failed depending on the order PostgreSQL combined rows — with `PARALLEL = SAFE` making that
/// a planner decision. **Widening the state, rather than deleting the operation, is what makes a
/// row aggregate expressible again**; the narrow state was the defect, not the aggregate.
///
/// # Encoding
///
/// [`Self::to_le_bytes`] and [`Self::from_le_bytes`] exist because a SQL transition state has to
/// survive being handed between parallel workers. The byte order is **stated here**, not
/// inherited from the host: this crate has already been bitten by a persisted hash whose bytes
/// came from `Hasher::write_i128`'s native endianness, and a transition state that decodes
/// differently on a big-endian replica is that defect wearing a different hat.
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
    /// [`MoneyError::DomainOverflow`] naming the offending term. Enforced per term, not assumed
    /// of the caller: checking only the total lets terms that were never money cancel into a
    /// plausible one — `[i128::MAX, -i128::MAX]` would otherwise sum to `Ok(0)`.
    ///
    /// Also if the accumulator itself leaves `I256`. Unreachable for a state this type built —
    /// each term is below 1e36 and `I256::MAX` is ~5.7e76, so that needs ~5.7e40 in-domain terms
    /// — but reachable for one that arrived through [`Self::from_le_bytes`].
    pub fn add_units(self, units: i128) -> Result<Self, MoneyError> {
        if !crate::domain::in_domain(units) {
            return Err(MoneyError::DomainOverflow { attempted_units: units });
        }
        match self.0.checked_add(I256::from(units)) {
            Some(acc) => Ok(Self(acc)),
            None => Err(MoneyError::DomainOverflow { attempted_units: narrow_saturating(self.0) }),
        }
    }

    /// Combine two partial sums.
    ///
    /// Associative and commutative, which is precisely what lets PostgreSQL combine parallel
    /// partials in whatever order its workers happen to finish and still produce one answer.
    ///
    /// # Errors
    /// [`MoneyError::DomainOverflow`] if the combined accumulator leaves `I256`. Unreachable for
    /// states this type built; reachable for one decoded from arbitrary bytes.
    pub fn merge(self, other: Self) -> Result<Self, MoneyError> {
        match self.0.checked_add(other.0) {
            Some(acc) => Ok(Self(acc)),
            None => Err(MoneyError::DomainOverflow { attempted_units: narrow_saturating(self.0) }),
        }
    }

    /// Narrow once, and check the domain once.
    ///
    /// # Errors
    /// [`MoneyError::DomainOverflow`] if the total is outside the domain, reporting the true
    /// figure where `i128` can hold it and a saturated bound where it cannot.
    pub fn finish(self) -> Result<i128, MoneyError> {
        let attempted = narrow_saturating(self.0);
        if crate::domain::in_domain(attempted) {
            Ok(attempted)
        } else {
            Err(MoneyError::DomainOverflow { attempted_units: attempted })
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
    /// handed out, so nothing left the ledger. (specs.md C5)
    ///
    /// This is **not** how you split a payment N ways: these shares will not sum back to the
    /// whole. Use [`Money::allocate`] for that.
    ///
    /// # Panics
    /// Never, here. The internal `expect`s cannot fire for any in-domain input — the quotient
    /// cannot exceed the dividend and the residue cannot reach the divisor. If one ever did,
    /// the domain invariant would already be broken, which is not a condition a caller can
    /// provoke. A [`Residue`](crate::Residue) taken via [`Division::take_residue`] does still detonate if it
    /// is dropped unabsorbed, in every profile — that is its contract, not this one's.
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
/// [`MoneyError::DomainOverflow`] if `units` is outside the domain.
///
/// This arm replaces a doc comment that stated the precondition and left it unenforced —
/// "never, **for any in-domain** `units`" was a caller obligation nothing checked, and the
/// function accepted `i128::MAX` and returned a quotient outside the domain.
///
/// # Panics
/// Never, now that the domain is checked rather than assumed: the quotient cannot exceed the
/// dividend and the residue cannot reach the divisor, so neither narrowing can fail.
pub fn div_int_units(units: i128, n: NonZeroU32, mode: Rounding) -> Result<UntaggedDivision, MoneyError> {
    if !crate::domain::in_domain(units) {
        return Err(MoneyError::DomainOverflow { attempted_units: units });
    }
    let (q, r) = div_round_i256(I256::from(units), I256::from(i128::from(n.get())), mode);
    // |q| <= |units| <= DOMAIN_MAX and |r| < n, so both conversions are total.
    let q = i128::try_from(q).expect("quotient magnitude cannot exceed the dividend");
    let r = i128::try_from(r).expect("residue magnitude is below the divisor");
    Ok(UntaggedDivision::new(q, r))
}

#[cfg(test)]
mod tests {
    use crate::arith::{UnitSum, sum_units};
    use crate::domain::{DOMAIN_MAX, MoneyError};
    use crate::iso::USD;
    use crate::money::Money;

    fn m(u: i128) -> Money<USD> {
        Money::<USD>::from_units(u).unwrap()
    }

    #[test]
    fn add_and_sub_are_exact() {
        assert_eq!((m(10_500_000_000_000) + m(2_250_000_000_000)).units(), 12_750_000_000_000);
        assert_eq!((m(1) - m(2)).units(), -1);
        assert_eq!((-m(5)).units(), -5);
    }

    #[test]
    fn checked_add_refuses_domain_overflow_loudly() {
        // The Decimal design returned Some() here after silently dropping a digit. (specs.md E3)
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
            Err(MoneyError::DomainOverflow { attempted_units: i128::MAX }),
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
    #[should_panic(expected = "money domain overflow")]
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
        // The exact R2-F4 failure the removed `Sum` had. Folding through the panicking `Add`,
        // the partial sum MAX+MAX leaves the domain even though the TOTAL is in it — so
        // `[MAX, MAX, -MAX]` panicked while `[MAX, -MAX, MAX]` returned MAX. Same multiset,
        // different order, different outcome. `try_sum` accumulates in I256 and checks once.
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
        // THE PROPERTY A PARALLEL PLAN NEEDS. PostgreSQL splits the rows across workers however
        // it likes, sums each part with the transition function, then merges the partials. Every
        // split must land on the answer `sum_units` gives for the whole list, or the total is a
        // property of the plan rather than of the data — which is precisely R2-F4.
        // Totals to DOMAIN_MAX - 4: two of the partial sums leave the domain, the total does not.
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
        assert!(matches!(e, MoneyError::DomainOverflow { attempted_units: i128::MAX }));
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
        assert!(huge.add_units(1).is_err(), "accumulator overflow must be Err");
        assert!(huge.merge(huge).is_err(), "merge overflow must be Err");
        assert!(huge.finish().is_err(), "a total that far out is not money");
    }

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
}
