//! The everyday path: balances, transfers, splitting a bill, and dividing money.
//!
//! Run with `cargo run -p kamu-money-core --example ledger`.
//!
//! The commented compile failures show constraints enforced by the type system.

use core::num::NonZeroU32;
use kamu_money_core::Money;
use kamu_money_core::Rounding;
use kamu_money_core::iso::{IDR, JPY, KWD, USD};

// Display uses the canonical text contract: trim trailing zeros to the ISO settlement exponent
// and never round.

fn main() {
    println!("== balances ==");

    // Two ways in. Both are checked: the domain is |units| <= 10^36 - 1, and nothing else
    // exists. There is no unchecked constructor to reach for.
    let opening = Money::<IDR>::try_from_major(5_000_000).expect("in domain");
    let fee = Money::<IDR>::try_from_units(1_500_000_000_000_000_000).expect("in domain"); // 1.5

    println!("  opening      {}", opening);
    println!("  fee          {}", fee);
    println!("  after fee    {}", opening - fee);

    // `try_sum` accumulates wide and checks the final total once, so intermediate ordering
    // cannot create a false overflow.
    let entries = [fee, fee, fee];
    let total = Money::<IDR>::try_sum(entries).expect("three fees stay in domain");
    println!("  3 x fee      {}", total);

    // Does not compile: currency identity lives in the type.
    //
    //     let usd = Money::<USD>::try_from_major(1).unwrap();
    //     let _ = opening + usd;
    //     //      ^^^^^^^^^^^^^ expected `Money<IDR>`, found `Money<USD>`
    //
    // Pinned by tests/ui/cross_currency_add.rs.

    println!("\n== splitting a bill: allocate() conserves, exactly ==");

    // The classic 10.00 / 3 problem. allocate() distributes the remainder rather than
    // dropping it, so the parts sum back to the whole for any valid weights.
    let bill = Money::<USD>::try_from_major(10).expect("in domain");
    let parts = bill.allocate(&[1, 1, 1]).expect("weights are valid");
    for (i, p) in parts.iter().enumerate() {
        println!("  share {}      {}", i + 1, *p);
    }
    let summed = Money::<USD>::try_sum(&parts).expect("allocation conserves, so the sum is the whole");
    println!("  sum          {}  (== {})", summed, bill);
    assert_eq!(summed, bill, "allocate must conserve");

    // Uneven weights work the same way, on an amount that does not divide cleanly.
    let pot = Money::<IDR>::try_from_units(1_000_000_000_000_000_001).expect("in domain");
    let split = pot.allocate(&[30, 70]).expect("weights are valid");
    println!("  30/70 of {}\n            ->  {}  +  {}", pot, split[0], split[1]);
    assert_eq!(Money::<IDR>::try_sum(&split).expect("allocation conserves"), pot);

    println!("\n== dividing: the quotient requires a residue decision ==");

    // `div_int` returns one decision-bearing value, not a tuple.
    let three = NonZeroU32::new(3).expect("nonzero");

    // (a) take the residue and post it yourself.
    let (share, residue) = bill.div_int(three, Rounding::TowardZero).take_residue();
    println!("  share        {}", share);
    println!("  residue      {} units  <- real money, handed back", residue.take_units());

    // (b) throw it away, on purpose, on the record. No Residue is ever constructed here.
    let share = bill.div_int(three, Rounding::HalfEven).discard_deliberately();
    println!("  discarded    {}  (residue explicitly discarded)", share);

    // (c) look without deciding. The Division is still undecided afterwards, and dropping it
    //     is safe precisely because no money escaped.
    let pending = bill.div_int(three, Rounding::TowardZero);
    println!("  peek         {} units pending", pending.residue_units());
    drop(pending);

    // Does not compile: tuple destructuring cannot hide the residue.
    //
    //     let (share, _) = bill.div_int(three, Rounding::TowardZero);
    //     //  ^^^^^^^^^^ expected `Division<USD>`, found `(_, _)`
    //
    // Pinned by tests/ui/residue_wildcard_destructure.rs.

    println!("\n== one amount, rendered per the currency's settlement exponent ==");
    let units = 10_500_000_000_000_000_000; // 10.5, whatever the currency
    println!(
        "  {}      {}       {}",
        Money::<USD>::try_from_units(units).expect("in domain"), // exp 2
        Money::<JPY>::try_from_units(units).expect("in domain"), // exp 0
        Money::<KWD>::try_from_units(units).expect("in domain"), // exp 3
    );
    let whole = 10_000_000_000_000_000_000;
    println!(
        "  {}      {}         {}",
        Money::<USD>::try_from_units(whole).expect("in domain"),
        Money::<JPY>::try_from_units(whole).expect("in domain"),
        Money::<KWD>::try_from_units(whole).expect("in domain"),
    );
    println!("  ^ trailing zeros trimmed, never below the settlement exponent, never rounded");
}
