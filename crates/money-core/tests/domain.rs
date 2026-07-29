use kamu_money_core::Money;
use kamu_money_core::Rounding;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::iso::USD;
use proptest::prelude::*;

proptest! {
    /// Weighted bands exercise accepted and rejected values on every run. Separate example tests
    /// pin exact boundaries that random sampling is unlikely to hit.
    #[test]
    fn prop_constructor_accepts_exactly_the_domain(
        u in prop_oneof![
            2 => -DOMAIN_MAX..=DOMAIN_MAX,
            1 => (DOMAIN_MAX + 1)..=i128::MAX,
            1 => i128::MIN..=(-DOMAIN_MAX - 1),
        ],
    ) {
        // Use an independent literal range rather than the implementation predicate.
        let inside = (-DOMAIN_MAX..=DOMAIN_MAX).contains(&u);
        prop_assert_eq!(Money::<USD>::try_from_units(u).is_ok(), inside);
    }

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
