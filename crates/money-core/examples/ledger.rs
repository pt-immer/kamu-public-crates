//! The everyday path: balances, transfers, splitting a bill, and dividing money.
//!
//! Run with `cargo run -p kamu-money-core --example ledger`.
//!
//! Read the `WILL NOT COMPILE` blocks as carefully as the code that runs — they are the point.
//! In a financial system the valuable property is not what you can express, it is what you
//! cannot.

use core::num::NonZeroU32;
use kamu_money_core::iso::{IDR, JPY, KWD, USD};
use kamu_money_core::money::Money;
use kamu_money_core::rounding::Rounding;

// Money implements Display, and this example used to hand-roll a formatter instead. That was
// the finding that put Display in the crate: a money format is a contract (C7), so a library
// that ships none forces every consumer to invent one, and two examples in this repo already
// had. The rule is: render at 18dp, strip trailing zeros, stop at the currency's ISO
// SETTLEMENT exponent, never round. See the last block for what that looks like.

fn main() {
    println!("== balances ==");

    // Two ways in. Both are checked: the domain is |units| <= 10^36 - 1, and nothing else
    // exists. There is no unchecked constructor to reach for.
    let opening = Money::<IDR>::from_major(5_000_000).expect("in domain");
    let fee = Money::<IDR>::from_units(1_500_000_000_000_000_000).expect("in domain"); // 1.5

    println!("  opening      {}", opening);
    println!("  fee          {}", fee);
    println!("  after fee    {}", opening - fee);

    // Summing is exact, and it is `try_sum`, not `.sum()`. The crate does not implement `Sum`:
    // a fold through `+` can leave the domain on a partial total that the final total returns
    // to, which made `.sum()` order-dependent (R2-F4). `try_sum` accumulates wide and checks
    // once, so it is a function of the values, not their order — and it returns a `Result`,
    // because a genuinely out-of-domain total is the one thing a sum of money can get wrong.
    let entries = [fee, fee, fee];
    let total = Money::<IDR>::try_sum(entries).expect("three fees stay in domain");
    println!("  3 x fee      {}", total);

    // ------------------------------------------------------------------------------------
    // WILL NOT COMPILE — the whole reason the currency lives in the type:
    //
    //     let usd = Money::<USD>::from_major(1).unwrap();
    //     let _ = opening + usd;
    //     //      ^^^^^^^^^^^^^ expected `Money<IDR>`, found `Money<USD>`
    //
    // No runtime check to forget, no error to handle, and no code path where an IDR balance
    // and a USD balance are added. Pinned by tests/ui/cross_currency_add.
    // ------------------------------------------------------------------------------------

    println!("\n== splitting a bill: allocate() conserves, exactly ==");

    // The classic 10.00 / 3 problem. allocate() distributes the remainder rather than
    // dropping it, so the parts sum back to the whole for ANY weights.
    let bill = Money::<USD>::from_major(10).expect("in domain");
    let parts = bill.allocate(&[1, 1, 1]);
    for (i, p) in parts.iter().enumerate() {
        println!("  share {}      {}", i + 1, *p);
    }
    let summed = Money::<USD>::try_sum(&parts).expect("allocation conserves, so the sum is the whole");
    println!("  sum          {}  (== {})", summed, bill);
    assert_eq!(summed, bill, "allocate must conserve");

    // Uneven weights work the same way, on an amount that does not divide cleanly.
    let pot = Money::<IDR>::from_units(1_000_000_000_000_000_001).expect("in domain");
    let split = pot.allocate(&[30, 70]);
    println!("  30/70 of {}\n            ->  {}  +  {}", pot, split[0], split[1]);
    assert_eq!(Money::<IDR>::try_sum(&split).expect("allocation conserves"), pot);

    println!("\n== dividing: the residue cannot be dropped by accident ==");

    // div_int returns ONE value. There is no tuple, so there is nothing to `let (x, _) =`.
    let three = NonZeroU32::new(3).expect("nonzero");

    // (a) take the residue and post it yourself.
    let (share, residue) = bill.div_int(three, Rounding::TowardZero).take_residue();
    println!("  share        {}", share);
    println!("  residue      {} units  <- real money, handed back", residue.take_units());

    // (b) throw it away, on purpose, on the record. No Residue is ever constructed here.
    let share = bill.div_int(three, Rounding::HalfEven).discard_deliberately();
    println!("  discarded    {}  (residue acknowledged and dropped)", share);

    // (c) look without deciding. The Division is still undecided afterwards, and dropping it
    //     is safe precisely because no money escaped.
    let pending = bill.div_int(three, Rounding::TowardZero);
    println!("  peek         {} units pending", pending.residue_units());
    drop(pending);

    // ------------------------------------------------------------------------------------
    // WILL NOT COMPILE — the pattern developers actually reach for, and the one rustc itself
    // suggests when you leave a variable unused:
    //
    //     let (share, _) = bill.div_int(three, Rounding::TowardZero);
    //     //  ^^^^^^^^^^ expected `Division<USD>`, found `(_, _)`
    //
    // It used to compile with no warning at all and panic at runtime. Now the money cannot
    // be extracted without saying what happens to the remainder.
    // Pinned by tests/ui/residue_wildcard_destructure.
    // ------------------------------------------------------------------------------------

    println!("\n== one amount, rendered per the currency's settlement exponent ==");
    let units = 10_500_000_000_000_000_000; // 10.5, whatever the currency
    println!(
        "  {}      {}       {}",
        Money::<USD>::from_units(units).expect("in domain"), // exp 2
        Money::<JPY>::from_units(units).expect("in domain"), // exp 0
        Money::<KWD>::from_units(units).expect("in domain"), // exp 3
    );
    let whole = 10_000_000_000_000_000_000;
    println!(
        "  {}      {}         {}",
        Money::<USD>::from_units(whole).expect("in domain"),
        Money::<JPY>::from_units(whole).expect("in domain"),
        Money::<KWD>::from_units(whole).expect("in domain"),
    );
    println!("  ^ trailing zeros trimmed, never below the settlement exponent, never rounded");
}
