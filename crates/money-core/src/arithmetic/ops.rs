//! The operator surface. Every impl `Money<C>` carries lives here, in one place.
//!
//! `+` and `-` panic on domain overflow, matching integer arithmetic. The message is read off
//! the error `try_add` returns, so the operator and the fallible method cannot describe one
//! refusal differently.

use crate::Money;
use crate::StaticCurrency;
use core::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// Panics on domain overflow, matching integer `Add`. Use [`Money::checked_add`] when overflow
/// is recoverable, or [`Money::try_add`] when the rejected total is worth reporting.
impl<C: StaticCurrency> Add for Money<C> {
    type Output = Self;
    #[inline]
    fn add(self, o: Self) -> Self {
        // The panic message reads the attempted total off the error rather than recomputing it,
        // so the operator and the fallible method cannot describe the same refusal differently.
        match self.try_add(o) {
            Ok(sum) => sum,
            Err(e) => panic!("{e}"),
        }
    }
}

impl<C: StaticCurrency> Sub for Money<C> {
    type Output = Self;
    #[inline]
    fn sub(self, o: Self) -> Self {
        match self.try_sub(o) {
            Ok(difference) => difference,
            Err(e) => panic!("{e}"),
        }
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

#[cfg(test)]
// This module's subject IS the operator surface, so `+` and `-` on `Money` are the thing under
// test rather than an unchecked shortcut, and `a + b` is the oracle they are checked against.
// The strategy halves the domain, so neither can overflow.
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use crate::Money;
    use crate::domain::DOMAIN_MAX;
    use crate::iso::USD;
    use proptest::prelude::*;

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
    fn the_operator_panics_with_exactly_what_try_add_reports() {
        let reported = m(DOMAIN_MAX).try_add(m(1)).unwrap_err().to_string();
        let panicked = std::panic::catch_unwind(|| m(DOMAIN_MAX) + m(1)).unwrap_err();
        let msg = panicked.downcast_ref::<String>().map_or("", String::as_str);
        assert_eq!(msg, reported, "the operator and the fallible method describe one refusal");
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

    proptest::proptest! {
    #[test]
    fn prop_add_never_rounds(a in -DOMAIN_MAX/2..=DOMAIN_MAX/2, b in -DOMAIN_MAX/2..=DOMAIN_MAX/2) {
        // Halving the range keeps the sum in-domain, so this is total.
        // The result is the exact integer sum at the fixed scale.
        let s = Money::<USD>::try_from_units(a).unwrap() + Money::<USD>::try_from_units(b).unwrap();
        prop_assert_eq!(s.units(), a + b);
    }

    #[test]
    fn prop_sub_is_the_inverse_of_add(a in -DOMAIN_MAX/2..=DOMAIN_MAX/2, b in -DOMAIN_MAX/2..=DOMAIN_MAX/2) {
        let x = Money::<USD>::try_from_units(a).unwrap();
        let y = Money::<USD>::try_from_units(b).unwrap();
        prop_assert_eq!((x + y - y).units(), a);
    }
    }
}
