use kamu_money_core::Money;
use kamu_money_core::Rounding;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::iso::USD;
use proptest::prelude::*;

proptest! {
    /// The bands are sampled EXPLICITLY rather than drawn from `i128::MIN..=i128::MAX`.
    /// The domain is ~1% of the i128 range, so a flat draw reaches "accepted" only by luck:
    /// measured, a mutant whose `try_from_units` rejected EVERY value still passed this property
    /// on 1 run in 5 at proptest's default 256 cases, and needed 584 draws to be caught once.
    /// A property that reaches only one branch of its own condition is decoration for the
    /// other branch. The exact boundaries (±DOMAIN_MAX, ±DOMAIN_MAX+1, i128::MIN) are pinned
    /// separately by `money.rs::construction_enforces_the_domain` — random sampling will
    /// never hit them, so neither test subsumes the other. Do not delete either as redundant.
    #[test]
    fn prop_constructor_accepts_exactly_the_domain(
        u in prop_oneof![
            2 => -DOMAIN_MAX..=DOMAIN_MAX,        // interior: must be ACCEPTED
            1 => (DOMAIN_MAX + 1)..=i128::MAX,    // above the domain: must be REJECTED
            1 => i128::MIN..=(-DOMAIN_MAX - 1),   // below the domain: must be REJECTED
        ],
    ) {
        // Deliberately an INDEPENDENT oracle: a literal range, not a call to `in_domain`.
        // Asserting `try_from_units` against the predicate it is implemented with would be the
        // `assert_eq!(EXPR, EXPR)` shape that stayed green for seven tasks while checking nothing.
        let inside = (-DOMAIN_MAX..=DOMAIN_MAX).contains(&u);
        prop_assert_eq!(Money::<USD>::try_from_units(u).is_ok(), inside);
    }

    #[test]
    fn prop_add_never_rounds(a in -DOMAIN_MAX/2..=DOMAIN_MAX/2, b in -DOMAIN_MAX/2..=DOMAIN_MAX/2) {
        // Halving the range keeps the sum in-domain, so this is total.
        // The claim: the result is the EXACT integer sum. No scale games. (contrast DESIGN.md E3)
        let s = Money::<USD>::try_from_units(a).unwrap() + Money::<USD>::try_from_units(b).unwrap();
        prop_assert_eq!(s.units(), a + b);
    }

    #[test]
    fn prop_sub_is_the_inverse_of_add(a in -DOMAIN_MAX/2..=DOMAIN_MAX/2, b in -DOMAIN_MAX/2..=DOMAIN_MAX/2) {
        let x = Money::<USD>::try_from_units(a).unwrap();
        let y = Money::<USD>::try_from_units(b).unwrap();
        prop_assert_eq!((x + y - y).units(), a, "exactness means this is EXACT, not approximate");
    }

    #[test]
    fn prop_div_int_conserves(u in -DOMAIN_MAX..=DOMAIN_MAX, n in 1u32..=1000, mi in 0usize..Rounding::ALL.len()) {
        // `Rounding::ALL.len()`, not a literal 7: a hardcoded bound would silently stop
        // covering a mode the day an eighth is added, and the test would stay green while
        // testing less. Same drift shape as the deleted `StaticCurrency::EXP`.
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
        // At most 50 values in +/-1e9, so the total is at most 5e10 -- far inside the domain,
        // and `try_sum` cannot error here. Its VALUE must equal the plain i128 fold.
        let ms: Vec<Money<USD>> = v.iter().map(|&u| Money::<USD>::try_from_units(u).unwrap()).collect();
        prop_assert_eq!(Money::<USD>::try_sum(&ms).unwrap().units(), v.iter().sum::<i128>());
    }
}
