use kamu_money_core::Money;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::errors::{AllocationError, AmountError};
use kamu_money_core::iso::USD;
use proptest::prelude::*;

fn m(u: i128) -> Money<USD> {
    Money::<USD>::try_from_units(u).unwrap()
}

#[test]
fn allocate_conserves_the_whole() {
    let parts = m(10_000_000_000_000_000_000).allocate(&[1, 1, 1]).unwrap();
    assert_eq!(parts.len(), 3);
    let sum: i128 = parts.iter().map(Money::units).sum();
    assert_eq!(sum, 10_000_000_000_000_000_000, "a naive split would lose a unit here");
    assert_eq!(parts[0].units(), 3_333_333_333_333_333_334, "the remainder lands on the first part");
}

#[test]
fn allocate_respects_weights() {
    let parts = m(10_000_000_000_000_000_000).allocate(&[3, 7]).unwrap();
    assert_eq!(parts[0].units(), 3_000_000_000_000_000_000);
    assert_eq!(parts[1].units(), 7_000_000_000_000_000_000);
}

#[test]
fn allocate_at_the_domain_top_does_not_overflow() {
    // units * weight here is ~4e45, which OVERFLOWS i128 (max 1.7e38).
    // If this passes, the I256 path is really being used. (DESIGN.md E11)
    let parts = m(DOMAIN_MAX).allocate(&[u32::MAX, 1]).unwrap();
    let sum: i128 = parts.iter().map(Money::units).sum();
    assert_eq!(sum, DOMAIN_MAX);
}

#[test]
fn allocate_handles_negative_and_tiny() {
    assert_eq!(m(1).allocate(&[1, 1, 1]).unwrap().iter().map(Money::units).sum::<i128>(), 1);
    assert_eq!(
        m(-10_000_000_000_000_000_000).allocate(&[1, 1, 1]).unwrap().iter().map(Money::units).sum::<i128>(),
        -10_000_000_000_000_000_000
    );
}

#[test]
fn split_conserves() {
    use core::num::NonZeroU32;
    let parts: Vec<_> = m(10_000_000_000_000_000_000).split(NonZeroU32::new(3).unwrap()).collect();
    assert_eq!(parts.iter().map(Money::units).sum::<i128>(), 10_000_000_000_000_000_000);
}

/// `split` computes equal weights directly instead of materialising `vec![1u32; n]`, which
/// let an externally-chosen `n` size three vectors at once. The rewrite must be
/// **indistinguishable** from what it replaced, not merely conserving — so this pins it
/// against the very expression it removed, across signs and awkward remainders.
///
/// Mutation-check, measured: swapping `div_euclid`/`rem_euclid` for `/` and `%` fails this
/// on every negative input, because truncation toward zero hands the front parts a unit the
/// arithmetic never produced.
#[test]
fn split_matches_the_allocation_it_replaced() {
    use core::num::NonZeroU32;
    for units in
        [0, 1, -1, 7, -7, 10_000_000_000_000_000_000, -10_000_000_000_000_000_000, DOMAIN_MAX, -DOMAIN_MAX]
    {
        for n in [1u32, 2, 3, 4, 7, 10, 97] {
            let count = NonZeroU32::new(n).unwrap();
            let split: Vec<_> = m(units).split(count).collect();
            let allocated = m(units).allocate(&vec![1u32; n as usize]).unwrap();
            assert_eq!(split, allocated, "units={units} n={n}");

            assert_eq!(split.len(), n as usize, "part count");
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

/// **The M2 fix, shown on the value that motivated it.**
///
/// `NonZeroU32` admits `u32::MAX`; the old eager implementation could reserve about 68.7 GB for
/// that count. The iterator remains constant-size, reports the exact count, and yields initial
/// parts without allocating.
///
/// **It deliberately does not try to make `split_collect` fail.** A real 68.7 GB `try_reserve_exact`
/// is not a reliable assertion: under Linux overcommit the reservation can succeed and hand back
/// address space, and the failure then arrives as the OOM killer partway through filling, which
/// is not a test result — it is a dead test runner. What M2 actually asked for, and what is
/// testable, is that a caller has a path which never makes the request at all.
#[test]
fn split_costs_nothing_at_the_part_count_that_motivated_it() {
    use core::num::NonZeroU32;
    let all = usize::try_from(u32::MAX).expect("64-bit target");
    let mut parts = m(DOMAIN_MAX).split(NonZeroU32::new(u32::MAX).unwrap());

    assert_eq!(parts.len(), all, "exact size, with nothing materialised");

    // `|p| p.units()`, not `Money::units`: this iterator yields by VALUE, and `units` takes
    // `&self`, so the point-free spelling the `Vec`-based tests use does not apply here.
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
        assert!(*units == base || *units == base + 1, "part {i} is {units}, expected {base} or {}", base + 1);
    }
}

/// M1 (2026-07-27, round 3): weights arriving from a request body, a config file or a database
/// row are ordinary service input, so the TYPED path needs a fallible form. Until now the only
/// fallible allocator was `allocate_units`, which returns raw `i128` -- so a caller either left
/// the typed API or repeated the conversion by hand, and the typed one panicked.
#[test]
fn allocate_reports_bad_weights_instead_of_panicking() {
    assert_eq!(
        m(100).allocate(&[]),
        Err(AllocationError::InvalidWeights { weights: 0 }),
        "empty weights are a value, not a panic"
    );
    assert_eq!(
        m(100).allocate(&[0, 0, 0]),
        Err(AllocationError::InvalidWeights { weights: 3 }),
        "all-zero weights have no meaningful distribution"
    );

    // The typed facade must preserve the raw kernel's distribution.
    for units in [0, 1, -1, 7, -7, DOMAIN_MAX, -DOMAIN_MAX] {
        for weights in [&[1u32, 1, 1][..], &[3, 1][..], &[1, 0, 2][..], &[5][..]] {
            let typed = m(units).allocate(weights).expect("weights are valid");
            let raw = kamu_money_core::advanced::arithmetic::allocate_units(units, weights).unwrap();
            assert_eq!(typed.iter().map(Money::units).collect::<Vec<_>>(), raw);
        }
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

/// The raw allocator reports both invalid weights and invalid amounts.
#[test]
fn the_runtime_allocator_refuses_bad_weights_without_panicking() {
    use kamu_money_core::advanced::arithmetic::allocate_units;

    assert_eq!(allocate_units(1, &[]), Err(AllocationError::InvalidWeights { weights: 0 }));
    assert_eq!(allocate_units(1, &[0, 0]), Err(AllocationError::InvalidWeights { weights: 2 }));
    // ...and the out-of-domain arm still reports the OTHER error, so the two are distinguishable.
    assert_eq!(
        allocate_units(DOMAIN_MAX + 1, &[1, 1]),
        Err(AllocationError::Amount(AmountError::out_of_domain(DOMAIN_MAX + 1)))
    );
    // A single non-zero weight among zeros is allocatable, so the check is "all zero", not "any zero".
    assert_eq!(allocate_units(10, &[0, 1, 0]).unwrap(), vec![0, 10, 0]);
}

/// R2-F1: a zero-weight recipient has no claim, so the truncation remainder must never land on
/// it. Conservation does NOT catch this — `[1, 0, 0]` sums to `1` exactly as `[0, 1, 0]` does —
/// so the property is asserted DIRECTLY, over positive and negative amounts, with exact vectors
/// for the cases the fix changes. (Standing lesson: an invariant test does not pin a
/// distribution; assert the distribution, not only its sum.)
#[test]
fn the_allocator_never_pays_a_zero_weight_recipient() {
    use kamu_money_core::advanced::arithmetic::allocate_units;

    // The measured defect: the whole odd unit used to land on the leading zero slot.
    assert_eq!(allocate_units(1, &[0, 1, 1]).unwrap(), vec![0, 1, 0]);
    assert_eq!(allocate_units(-1, &[0, 1, 1]).unwrap(), vec![0, -1, 0]);
    // Interleaved zeros are skipped too: 7 over weights (3, 3) leaves a 1-unit remainder that
    // must reach the FIRST POSITIVE slot (index 1), never the zero at index 0.
    assert_eq!(allocate_units(7, &[0, 3, 0, 3, 0]).unwrap(), vec![0, 4, 0, 3, 0]);

    // The general property, swept: every zero-weight index gets exactly 0, and the total is
    // still conserved, for positive and negative amounts across several zero patterns.
    for units in [1_i128, 7, 100, 999_999, -1, -7, -100, DOMAIN_MAX] {
        for weights in
            [&[0u32, 1, 1][..], &[1, 0, 1], &[1, 1, 0], &[0, 3, 0, 3, 0], &[0, 0, 1], &[7, 0, 0, 2, 0, 1]]
        {
            let parts = allocate_units(units, weights).unwrap();
            for (i, &w) in weights.iter().enumerate() {
                assert!(
                    w != 0 || parts[i] == 0,
                    "units={units} weights={weights:?}: paid {} to the zero-weight slot at {i}",
                    parts[i]
                );
            }
            assert_eq!(parts.iter().sum::<i128>(), units, "units={units} weights={weights:?}: not conserved");
        }
    }
}

proptest! {
    /// The contract, for all inputs: allocation NEVER creates or destroys money.
    #[test]
    fn prop_allocate_always_conserves(
        units in -DOMAIN_MAX..=DOMAIN_MAX,
        weights in prop::collection::vec(1u32..=1_000_000, 1..12),
    ) {
        let parts = m(units).allocate(&weights).unwrap();
        prop_assert_eq!(parts.len(), weights.len());
        let sum: i128 = parts.iter().map(Money::units).sum();
        prop_assert_eq!(sum, units);
    }
}
