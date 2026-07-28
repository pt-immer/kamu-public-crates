//! The small end of `Rate`, measured. (DESIGN.md C6)
//!
//! At `SCALE = 18` a rate is a count of `1e-18`, so a rate of magnitude `1e-13` has only
//! `1e5 = 100000` units behind it — five decimal digits of headroom, not the eighteen the
//! scale advertises. Because there is no `inverse()`, **every pair is stored in both
//! directions**, so the tiny counter-direction of a hyperinflation quote is mandatory rather
//! than hypothetical. This measures what survives there.

use kamu_money_core::advanced::domain::POW10_SCALE;
use kamu_money_core::iso::{IDR, USD};
use kamu_money_core::{Money, Rate, Rounding};

/// Significant decimal digits available to a rate of a given magnitude.
fn significant_digits(units: i128) -> u32 {
    units.unsigned_abs().checked_ilog10().map_or(0, |d| d + 1)
}

/// The headroom, per decade. This is the table C6's small-end question asked for.
#[test]
fn the_significant_digits_available_at_each_magnitude() {
    // (rate magnitude as a power of ten, units behind it, significant digits)
    let rows: &[(i32, i128, u32)] = &[
        (0, POW10_SCALE, 19), // 1.0
        (-3, 1_000_000_000_000_000, 16),
        (-6, 1_000_000_000_000, 13),
        (-9, 1_000_000_000, 10),
        (-13, 100_000, 6), // the case the contract named
        (-15, 1_000, 4),
        (-17, 10, 2),
        (-18, 1, 1), // one unit: the floor
    ];
    for &(exponent, units, expected) in rows {
        assert_eq!(significant_digits(units), expected, "rate 1e{exponent} has {units} units");
        assert!(Rate::<USD, IDR>::try_from_units(units).is_ok(), "1e{exponent} must be representable");
    }
}

/// **The contract's number was right.** A rate of `1e-13` holds six digits, so the seventh is
/// not merely imprecise — it does not exist, and `try_from_units` cannot round to it.
#[test]
fn a_rate_near_1e_minus_13_carries_six_significant_digits() {
    let rate = 100_000i128; // 1e-13 at scale 18
    assert_eq!(significant_digits(rate), 6);

    // The next representable value up is one part in 100_000 away: ~1e-5 relative resolution.
    let next = rate + 1;
    assert_eq!(significant_digits(next), 6);
    let relative_step = 1.0f64 / (rate as f64);
    assert!((relative_step - 1e-5).abs() < 1e-9, "resolution at 1e-13 is {relative_step}, expected ~1e-5");
}

/// The floor: one unit is a usable rate, and it converts without collapsing to zero — but the
/// money must be large enough to survive it. This is the honest limit.
#[test]
fn the_smallest_representable_rate_still_converts() {
    let smallest = Rate::<USD, IDR>::try_from_units(1).expect("1e-18 is in domain");

    // A big enough amount survives: 1e18 units * 1e-18 = 1 unit.
    let big = Money::<USD>::try_from_units(POW10_SCALE).expect("in domain");
    let out = big.convert(smallest, Rounding::TowardZero).expect("stays in domain");
    assert_eq!(out.units(), 1, "1.0 USD at 1e-18 is one IDR unit");

    // Anything smaller rounds to nothing — named, not hidden.
    let small = Money::<USD>::try_from_units(POW10_SCALE - 1).expect("in domain");
    let gone = small.convert(smallest, Rounding::TowardZero).expect("stays in domain");
    assert_eq!(
        gone.units(),
        0,
        "below 1.0 USD, a 1e-18 rate rounds to zero — the rate has no digits left to carry it"
    );
}

/// A realistic hyperinflation counter-direction, end to end. The forward rate is huge and the
/// reverse is tiny, and both must be storable because there is no `inverse()`.
#[test]
fn both_directions_of_a_hyperinflation_pair_are_representable() {
    // Forward: 1 USD = 3,000,000 IDR-like units.
    let forward = Rate::<USD, IDR>::try_from_units(3_000_000 * POW10_SCALE).expect("in domain");
    // Reverse: 1/3e6 = 3.3333...e-7, which at scale 18 is 333_333_333_333 units.
    let reverse = Rate::<IDR, USD>::try_from_units(333_333_333_333).expect("in domain");

    assert_eq!(significant_digits(reverse.units()), 12, "12 digits survive");

    // A round trip loses at most the truncation, and it is bounded rather than silent.
    let start = Money::<USD>::try_from_major(1).expect("in domain");
    let there = start.convert(forward, Rounding::TowardZero).expect("in domain");
    let back = there.convert(reverse, Rounding::TowardZero).expect("in domain");

    let drift = (start.units() - back.units()).abs();
    assert!(
        drift < POW10_SCALE / 1_000_000,
        "round-trip drift {drift} units must stay below 1e-6 of a major unit"
    );
}
