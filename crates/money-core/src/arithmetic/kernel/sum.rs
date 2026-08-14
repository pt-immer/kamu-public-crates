//! Wide accumulation: the domain is checked once, on the way out.

use crate::errors::AmountError;
use ethnum::I256;

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
/// [`AmountError::WideOutOfDomain`], carrying the exact total, if that total is wider than
/// `i128`.
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
        if !crate::domain::in_domain(units) {
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
    /// [`AmountError::WideOutOfDomain`] when it is exact but too wide to be one.
    ///
    /// Both carry the total. Refusing a value this type is holding is the point; forgetting it
    /// on the way out is not, and 171 terms at the domain edge is all it takes to leave `i128`.
    ///
    /// The total travels as an error payload rather than as money the caller must resolve.
    /// Nothing bounds the overshoot, so it is representable as [`Residue`](crate::Residue) only
    /// sometimes, which is why no [`Division`](crate::Division)-style accessor exists for it.
    pub fn finish(self) -> Result<i128, AmountError> {
        let Ok(attempted) = i128::try_from(self.0) else {
            return Err(AmountError::wide_out_of_domain(self.to_le_bytes()));
        };
        if crate::domain::in_domain(attempted) {
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

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::arithmetic::{UnitSum, sum_units};
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AmountError;
    use crate::iso::USD;
    use ethnum::I256;

    fn m(u: i128) -> Money<USD> {
        Money::<USD>::try_from_units(u).unwrap()
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
        // Nothing exact exists to report for these two: the accumulator itself overflowed.
        assert_eq!(huge.add_units(1), Err(AmountError::ArithmeticOverflow));
        assert_eq!(huge.merge(huge), Err(AmountError::ArithmeticOverflow));
        // `finish` is different. The state is intact and holds a total; it is merely too wide,
        // so the refusal carries it rather than replacing it with a bare variant.
        assert_eq!(huge.finish(), Err(AmountError::wide_out_of_domain(huge.to_le_bytes())));
    }

    #[test]
    fn a_total_too_wide_for_i128_is_refused_with_the_total_still_attached() {
        // 171 amounts at the domain edge is all it takes: 171 * (10^36 - 1) > i128::MAX. The
        // sum is exact and known, and the point of the variant is that it survives the refusal.
        let terms = core::iter::repeat_n(DOMAIN_MAX, 171);
        let expected = I256::from(DOMAIN_MAX) * I256::from(171i128);

        let Err(AmountError::WideOutOfDomain { attempted_units }) = sum_units(terms) else {
            panic!("a total wider than i128 must be refused as WideOutOfDomain");
        };
        assert_eq!(
            I256::from_le_bytes(attempted_units),
            expected,
            "the refusal must carry the exact total, not a rounded or truncated one"
        );

        // Positive control: one term fewer still narrows, so 171 is the real threshold rather
        // than an arbitrary count that happens to fail.
        assert!(
            matches!(sum_units(core::iter::repeat_n(DOMAIN_MAX, 170)), Err(AmountError::OutOfDomain { .. })),
            "170 terms still fit i128, so they must be refused for the domain, not the width"
        );
    }

    #[test]
    fn a_refused_totals_excess_is_not_uniformly_representable_as_money() {
        // Why summation returns a `Result` and not a `Division`-shaped product type.
        //
        // `Division` can hand its residue back unconditionally: `|residue| < n`, so it is
        // always inside the domain. A summation excess has no such bound. Two terms at the
        // edge overshoot by exactly DOMAIN_MAX, which IS a `Residue`; 171 overshoot by more
        // than `i128` can hold, which is not. So `Overflow<C>` could not offer the
        // unconditional `take_excess() -> Residue<C>` that makes `Division` worth having —
        // the accessor would be fallible, reintroducing the `Result` it set out to remove.
        let excess_of = |count: usize| -> I256 {
            let Err(error) = sum_units(core::iter::repeat_n(DOMAIN_MAX, count)) else {
                panic!("{count} terms at the domain edge must be refused");
            };
            let total = match error {
                AmountError::WideOutOfDomain { attempted_units } => I256::from_le_bytes(attempted_units),
                AmountError::OutOfDomain { attempted_units } => I256::from(attempted_units),
                other => panic!("an exact total must be reported, got {other:?}"),
            };
            total - I256::from(DOMAIN_MAX)
        };

        let representable = |excess: I256| match i128::try_from(excess) {
            // Too wide for the storage type, so far outside the money domain.
            Err(_) => false,
            Ok(units) => crate::Residue::<USD>::try_from_units(units).is_ok(),
        };

        // Two terms overshoot by exactly DOMAIN_MAX. Representable — the refusal is about the
        // total, not about the excess being unrepresentable.
        let small = excess_of(2);
        assert_eq!(small, I256::from(DOMAIN_MAX));
        assert!(representable(small), "a small overshoot IS a valid residue");

        // 170 terms still narrow to i128; 171 do not. Both overshoot the domain by more than
        // it can hold, so neither excess is money.
        for count in [170usize, 171, 400] {
            assert!(
                !representable(excess_of(count)),
                "{count} terms overshoot by more than the domain holds"
            );
        }
    }
}
