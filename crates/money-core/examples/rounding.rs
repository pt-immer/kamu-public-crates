//! Rounding modes and the residue, built around one equation.
//!
//! Run with `cargo run -p kamu-money-core --example rounding`.
//!
//! ```text
//! quotient * divisor + residue = original
//! ```
//!
//! That identity holds for every mode, and it is what makes a `Residue` an accounting artefact
//! rather than a leftover fraction. A residue is not "the part that did not fit". It is the
//! exact correction needed *after* the quotient was rounded, so it is **signed**: positive when
//! the rounded quotient accounts for too little, negative when it accounts for too much.
//!
//! Everything below works in canonical units — the smallest amount that exists, `1e-18` USD —
//! and uses tiny numbers so the arithmetic can be checked by eye. A later section repeats it on
//! a real `$10.00`.
//!
//! Every number this file narrates is derived from the values beside it, and every property it
//! states is asserted rather than reported. A printed `false` that still exits 0 is a silent
//! failure, and a hand-written count is correct only until the day it is not.
//!
//! `ledger.rs` covers the *shape* of the division API: the three ways to resolve a `Division`,
//! and the destructure that does not compile. This example is about which answer each mode
//! gives, and why.

use core::num::NonZeroU32;
use kamu_money_core::iso::USD;
use kamu_money_core::{Money, Rounding};

fn nz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("nonzero literal")
}

fn usd(units: i128) -> Money<USD> {
    Money::<USD>::try_from_units(units).expect("well inside the domain")
}

/// Divide, then resolve the division into `(quotient units, residue units)`.
fn divide(units: i128, divisor: u32) -> impl Fn(Rounding) -> (i128, i128) {
    move |mode| {
        let (quotient, residue) = usd(units).div_int(nz(divisor), mode).take_residue();
        (quotient.units(), residue.take_units())
    }
}

/// `2 * 2 + 1 = 5`, written out so the identity can be read rather than trusted.
fn verify(quotient: i128, divisor: u32, residue: i128) -> String {
    let op = if residue < 0 { '-' } else { '+' };
    let total = quotient * i128::from(divisor) + residue;
    format!("{quotient} * {divisor} {op} {} = {total}", residue.abs())
}

/// One `original / divisor` across all seven modes. Returns the quotient each mode chose.
///
/// `answer` is the whole clause, not just the digits: `5 / 2` is `2.5` exactly, while `8 / 3`
/// has no finite decimal at all. In this crate "exact" is a claim, so the caller states it.
fn table(units: i128, divisor: u32, answer: &str) -> Vec<i128> {
    let d = divide(units, divisor);
    println!("  {units} / {divisor} = {answer}\n");
    println!("  {:<22} {:>8} {:>8}   {:<20}", "mode", "quotient", "residue", "check");
    println!("  {}", "-".repeat(64));

    let mut quotients = Vec::with_capacity(Rounding::ALL.len());
    for mode in Rounding::ALL {
        let (quotient, residue) = d(*mode);
        assert_eq!(quotient * i128::from(divisor) + residue, units, "identity must hold");
        println!(
            "  {:<22} {quotient:>8} {residue:>+8}   {}",
            mode.as_str(),
            verify(quotient, divisor, residue)
        );
        quotients.push(quotient);
    }
    quotients
}

/// How many of the seven modes chose each distinct quotient, lowest candidate first.
///
/// Derived from the table that was just printed, so the sentence under it cannot disagree with
/// the rows above it.
fn tally(quotients: &[i128]) -> Vec<(i128, usize)> {
    let mut candidates = quotients.to_vec();
    candidates.sort_unstable();
    candidates.dedup();
    candidates
        .into_iter()
        .map(|candidate| (candidate, quotients.iter().filter(|q| **q == candidate).count()))
        .collect()
}

fn join<T>(items: &[(i128, usize)], separator: &str, render: impl Fn(&(i128, usize)) -> T) -> String
where
    T: std::fmt::Display,
{
    items.iter().map(|item| render(item).to_string()).collect::<Vec<_>>().join(separator)
}

fn main() {
    // ===================================================================================
    println!("== the equation ==\n");
    println!("      residue = original - (quotient * divisor)");
    println!("  so  quotient * divisor + residue = original\n");
    println!("  5 / 2 is 2.5 exactly, and a quotient must be whole. Two answers are possible,");
    println!("  and each one needs a different correction to get back to 5:\n");

    let five_halved = divide(5, 2);
    for mode in [Rounding::TowardZero, Rounding::AwayFromZero] {
        let (quotient, residue) = five_halved(mode);
        println!("    quotient {quotient}  ->  residue {residue:+}   {}", verify(quotient, 2, residue));
    }
    println!("\n  Positive residue: the quotient accounted for too little, money is still owed out.");
    println!("  Negative residue: it accounted for too much, and that much is owed back.");

    // ===================================================================================
    println!("\n== a tie, positive: 5 / 2 ==\n");
    let positive_tie = table(5, 2, "2.5 exactly");
    let split = tally(&positive_tie);
    assert_eq!(split.len(), 2, "a tie leaves exactly two candidates to choose between");
    println!(
        "\n  Candidates are {}, equally near: {}.",
        join(&split, " and ", |(candidate, _)| *candidate),
        join(&split, ", ", |(candidate, count)| format!("{count} take {candidate}")),
    );

    // ===================================================================================
    println!("\n== the same tie, negative: -5 / 2 ==\n");
    table(-5, 2, "-2.5 exactly");
    println!("\n  The signs reverse, and this is arithmetic rather than a special case:");
    println!("    -2 * 2 = -4, so you still need -1 to reach -5");
    println!("    -3 * 2 = -6, so you need +1 to reach -5");
    println!("  Note floor and ceil have SWAPPED relative to toward_zero and away_from_zero.");

    // ===================================================================================
    println!("\n== not a tie: 8 / 3 ==\n");
    let not_a_tie = table(8, 3, "2.666..., which no finite decimal can write");
    // The three half_* modes are exactly the ones a non-tie cannot tell apart, so the claim
    // below is checked rather than asserted in prose.
    let half_modes = [Rounding::HalfEven, Rounding::HalfAwayFromZero, Rounding::HalfTowardZero];
    let half_quotients: Vec<i128> = half_modes
        .iter()
        .map(|mode| {
            let index = Rounding::ALL.iter().position(|m| m == mode).expect("mode is in ALL");
            not_a_tie[index]
        })
        .collect();
    assert!(
        half_quotients.windows(2).all(|pair| pair[0] == pair[1]),
        "a non-tie must leave the half_* modes indistinguishable"
    );
    println!("\n  All three half_* modes agree here, because 2.666 is plainly nearer 3 and there");
    println!("  is no tie to break. That is why a test built on a non-tie proves very little:");
    println!("  it cannot distinguish half_even from half_away_from_zero at all.");

    // ===================================================================================
    println!("\n== half_even: why it alternates ==\n");
    println!("  It picks the nearest, and on a tie the EVEN candidate — which is sometimes the");
    println!("  lower one and sometimes the higher one:\n");
    println!("  {:<12} {:>10} {:>10}   which candidate", "division", "quotient", "residue");
    println!("  {}", "-".repeat(58));

    let mut took_lower = 0_u32;
    let mut took_higher = 0_u32;
    for n in [3_i128, 5, 7, 9] {
        let (quotient, residue) = divide(n, 2)(Rounding::HalfEven);
        // The two candidates for an odd n over 2, and the claim the column makes.
        let lower = n / 2;
        assert!(quotient == lower || quotient == lower + 1, "the quotient is one of the two");
        assert_eq!(quotient % 2, 0, "half_even must land on the even candidate");
        let which = if quotient == lower {
            took_lower += 1;
            "the lower"
        } else {
            took_higher += 1;
            "the higher"
        };
        println!("  {:<12} {quotient:>10} {residue:>+10}   {which}", format!("{n} / 2"));
    }
    // "Sometimes the lower one and sometimes the higher one" is the whole point of the section,
    // so it fails here rather than reading as true above.
    assert!(took_lower > 0 && took_higher > 0, "half_even must be seen taking both directions");

    println!("\n  So the residue alternates sign as the quotient alternates direction. Over many");
    println!("  rows the corrections cancel instead of accumulating, which is the entire reason");
    println!("  half_even is the IEEE-754 default.");

    // ===================================================================================
    println!("\n== the two directed modes have a guarantee ==\n");
    println!("  floor never overshoots, so quotient * divisor <= original, so its residue can");
    println!("  only ever be zero or positive. ceil is the mirror: never below zero.\n");

    // Both ends of each range, not just the bounded one. Reporting only floor's minimum would
    // print `0` for a sweep in which every residue was zero — the same output a vacuous sweep
    // gives, and the assertion below would pass just as happily. The far end is what shows the
    // bound is reached rather than merely respected.
    let mut floor_range = (i128::MAX, i128::MIN);
    let mut ceil_range = (i128::MAX, i128::MIN);
    let mut nonzero = 0_u32;
    let mut checks = 0_u32;

    for units in [-9_i128, -8, -5, -3, -1, 0, 1, 3, 5, 8, 9, 1_000, -1_000] {
        for divisor in [1_u32, 2, 3, 4, 7, 100] {
            let d = divide(units, divisor);

            let (_, floor_r) = d(Rounding::Floor);
            let (_, ceil_r) = d(Rounding::Ceil);
            assert!(floor_r >= 0, "floor residue must never be negative");
            assert!(ceil_r <= 0, "ceil residue must never be positive");
            floor_range = (floor_range.0.min(floor_r), floor_range.1.max(floor_r));
            ceil_range = (ceil_range.0.min(ceil_r), ceil_range.1.max(ceil_r));
            if floor_r != 0 || ceil_r != 0 {
                nonzero += 1;
            }

            // A residue that is zero says nothing about sign, so the sign claims are made only
            // where there is a sign to make them about.
            let (_, toward_r) = d(Rounding::TowardZero);
            let (_, away_r) = d(Rounding::AwayFromZero);
            if toward_r != 0 {
                assert_eq!(
                    toward_r.signum(),
                    units.signum(),
                    "toward_zero's residue must share the sign of the amount ({units} / {divisor})"
                );
            }
            if away_r != 0 {
                assert_ne!(
                    away_r.signum(),
                    units.signum(),
                    "away_from_zero's residue must oppose the sign of the amount ({units} / {divisor})"
                );
            }

            checks += 1;
        }
    }
    assert!(nonzero > 0, "a sweep with no rounding at all would prove nothing about the bounds");

    println!("  Swept {checks} amount/divisor pairs, {nonzero} of which actually rounded:");
    println!("    floor residues:  all >= 0   (seen: {} .. {})", floor_range.0, floor_range.1);
    println!("    ceil residues:   all <= 0   (seen: {} .. {})", ceil_range.0, ceil_range.1);
    println!("    toward_zero:     residue shares the sign of the amount   (asserted)");
    println!("    away_from_zero:  residue opposes it                      (asserted)");
    println!("\n  Both ranges reach zero and reach away from it, so the bounds are tight rather");
    println!("  than vacuous. toward_zero keeps the shortfall on the same side as the amount;");
    println!("  away_from_zero always overshoots, so its correction points the other way.");

    // ===================================================================================
    println!("\n== on real money: $10.00 / 3 ==\n");

    let ten = Money::<USD>::try_from_major(10).expect("well inside the domain");
    println!("  {ten} is {} canonical units.", ten.units());
    println!("  The exact answer, 3.333..., needs more than 18 decimals, so it must round.\n");
    println!("  {:<22} {:<28} {:>9}", "mode", "quotient", "residue");
    println!("  {}", "-".repeat(62));
    for mode in Rounding::ALL {
        let (quotient, residue) = ten.div_int(nz(3), *mode).take_residue();
        let residue_units = residue.take_units();
        assert_eq!(quotient.units() * 3 + residue_units, ten.units());
        println!("  {:<22} {:<28} {residue_units:>+9}", mode.as_str(), quotient.to_string());
    }

    let (low, low_residue) = ten.div_int(nz(3), Rounding::TowardZero).take_residue();
    let (high, high_residue) = ten.div_int(nz(3), Rounding::Ceil).take_residue();
    let (low, high) = (low.units(), high.units());
    let (low_residue, high_residue) = (low_residue.take_units(), high_residue.take_units());
    println!(
        "\n  Why is the rounded-up residue {high_residue} and not {}? Multiply the shares out:",
        high_residue + 1
    );
    println!("    {low} * 3 = {}   -> short by {}, residue {low_residue:+}", low * 3, low_residue.abs());
    println!("    {high} * 3 = {}   -> over by {},  residue {high_residue:+}", high * 3, high_residue.abs());
    println!("  Three shares each one unit larger overshoot the total by three, and the total");
    println!("  was already one short — so the correction is {high_residue}, not {}.", high_residue + 1);
    assert_eq!(high, low + 1, "ceil is one canonical unit above toward_zero here");
    assert_eq!(high_residue, low_residue - 3, "one more unit per share costs three units of residue");

    // ===================================================================================
    println!("\n== div_int is not how you split a payment ==\n");

    let parts: Vec<Money<USD>> = ten.split(nz(3)).collect();
    assert_eq!(Money::<USD>::try_sum(&parts).expect("a split stays in domain"), ten);
    println!("  div_int answers \"what is one share worth\", and hands you a correction to file.");
    println!("  If the money is actually going to three people, use split() or allocate():\n");
    for (i, part) in parts.iter().enumerate() {
        println!("    part {}  {part}", i + 1);
    }
    println!("    total   {ten}   <- sums back exactly, no residue produced");
    println!("\n  The leftover units are spread across the parts instead of handed back. Nothing");
    println!("  is left to account for, because nothing left the total.");

    // ===================================================================================
    println!("\n== mental model ==\n");
    println!("      residue = original - (rounded quotient * divisor)\n");
    println!("    rounded too small  ->  residue positive");
    println!("    rounded too large  ->  residue negative");
    println!("    divided exactly    ->  residue zero");
    println!("    floor              ->  residue always >= 0");
    println!("    ceil               ->  residue always <= 0");
    println!("\n  The residue is the accounting proof of what the rounding mode moved, instead of");
    println!("  a fraction that quietly disappears and reappears later as an audit finding.");
}
