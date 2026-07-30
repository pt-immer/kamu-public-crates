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
//! and uses tiny numbers so the arithmetic can be checked by eye. The final section repeats it
//! on a real `$10.00`.

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

/// One `original / divisor` across all seven modes.
fn table(units: i128, divisor: u32, exact: &str) {
    let d = divide(units, divisor);
    println!("  {units} / {divisor} = {exact} exactly\n");
    println!("  {:<22} {:>8} {:>8}   {:<20}", "mode", "quotient", "residue", "check");
    println!("  {}", "-".repeat(64));
    for mode in Rounding::ALL {
        let (quotient, residue) = d(*mode);
        assert_eq!(quotient * i128::from(divisor) + residue, units, "identity must hold");
        println!(
            "  {:<22} {quotient:>8} {residue:>+8}   {}",
            mode.as_str(),
            verify(quotient, divisor, residue)
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    table(5, 2, "2.5");
    println!("\n  Candidates are 2 and 3, equally near. Four modes take 2, three take 3.");

    // ===================================================================================
    println!("\n== the same tie, negative: -5 / 2 ==\n");
    table(-5, 2, "-2.5");
    println!("\n  The signs reverse, and this is arithmetic rather than a special case:");
    println!("    -2 * 2 = -4, so you still need -1 to reach -5");
    println!("    -3 * 2 = -6, so you need +1 to reach -5");
    println!("  Note floor and ceil have SWAPPED relative to toward_zero and away_from_zero.");

    // ===================================================================================
    println!("\n== not a tie: 8 / 3 ==\n");
    table(8, 3, "2.666...");
    println!("\n  All three half_* modes agree here, because 2.666 is plainly nearer 3 and there");
    println!("  is no tie to break. That is why a test built on a non-tie proves very little:");
    println!("  it cannot distinguish half_even from half_away_from_zero at all.");

    // ===================================================================================
    println!("\n== half_even: why it alternates ==\n");
    println!("  It picks the nearest, and on a tie the EVEN candidate — which is sometimes the");
    println!("  lower one and sometimes the higher one:\n");
    println!("  {:<12} {:>10} {:>10}   even candidate", "division", "quotient", "residue");
    println!("  {}", "-".repeat(56));
    for n in [3_i128, 5, 7, 9] {
        let (quotient, residue) = divide(n, 2)(Rounding::HalfEven);
        println!("  {:<12} {quotient:>10} {residue:>+10}   {quotient} is even", format!("{n} / 2"));
    }
    println!("\n  So the residue alternates sign as the quotient alternates direction. Over many");
    println!("  rows the corrections cancel instead of accumulating, which is the entire reason");
    println!("  half_even is the IEEE-754 default.");

    // ===================================================================================
    println!("\n== the two directed modes have a guarantee ==\n");
    println!("  floor never overshoots, so quotient * divisor <= original, so its residue can");
    println!("  only ever be zero or positive. ceil is the mirror: never below zero.\n");

    let mut floor_min = i128::MAX;
    let mut ceil_max = i128::MIN;
    let mut toward_same_sign = true;
    let mut away_opposite_sign = true;
    let mut checks = 0_u32;

    for units in [-9_i128, -8, -5, -3, -1, 0, 1, 3, 5, 8, 9, 1_000, -1_000] {
        for divisor in [1_u32, 2, 3, 4, 7, 100] {
            let d = divide(units, divisor);

            let (_, floor_r) = d(Rounding::Floor);
            let (_, ceil_r) = d(Rounding::Ceil);
            assert!(floor_r >= 0, "floor residue must never be negative");
            assert!(ceil_r <= 0, "ceil residue must never be positive");
            floor_min = floor_min.min(floor_r);
            ceil_max = ceil_max.max(ceil_r);

            let (_, toward_r) = d(Rounding::TowardZero);
            let (_, away_r) = d(Rounding::AwayFromZero);
            if toward_r != 0 && toward_r.signum() != units.signum() {
                toward_same_sign = false;
            }
            if away_r != 0 && away_r.signum() == units.signum() {
                away_opposite_sign = false;
            }

            checks += 1;
        }
    }

    println!("  Swept {checks} amount/divisor pairs:");
    println!("    floor residues:  all >= 0   (smallest seen: {floor_min})");
    println!("    ceil residues:   all <= 0   (largest seen:  {ceil_max})");
    println!("    toward_zero:     residue always shares the sign of the amount: {toward_same_sign}");
    println!("    away_from_zero:  residue always opposes it:                    {away_opposite_sign}");
    println!("\n  toward_zero keeps the shortfall on the same side as the amount; away_from_zero");
    println!("  always overshoots, so its correction points the other way.");

    // ===================================================================================
    println!("\n== on real money: $10.00 / 3 ==\n");

    let ten = Money::<USD>::try_from_major(10)?;
    println!("  ${} is {} canonical units.", 10, ten.units());
    println!("  The exact answer, 3.333..., needs more than 18 decimals, so it must round.\n");
    println!("  {:<22} {:<28} {:>9}", "mode", "quotient", "residue");
    println!("  {}", "-".repeat(62));
    for mode in Rounding::ALL {
        let (quotient, residue) = ten.div_int(nz(3), *mode).take_residue();
        let residue_units = residue.take_units();
        assert_eq!(quotient.units() * 3 + residue_units, ten.units());
        println!("  {:<22} {:<28} {residue_units:>+9}", mode.as_str(), quotient.to_string());
    }

    let low = ten.div_int(nz(3), Rounding::TowardZero).discard_deliberately();
    let high = ten.div_int(nz(3), Rounding::Ceil).discard_deliberately();
    println!("\n  Why is the rounded-up residue -2 and not -1? Multiply the shares out:");
    println!("    {} * 3 = {}   -> 1 unit short, residue +1", low.units(), low.units() * 3);
    println!("    {} * 3 = {}   -> 2 units over,  residue -2", high.units(), high.units() * 3);
    println!("  Three shares each one unit larger overshoot the total by three, and the total");
    println!("  was already one short — so the correction is -2, not -1.");

    // ===================================================================================
    println!("\n== div_int is not how you split a payment ==\n");

    let parts: Vec<Money<USD>> = ten.split(nz(3)).collect();
    assert_eq!(Money::<USD>::try_sum(&parts)?, ten);
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

    Ok(())
}
