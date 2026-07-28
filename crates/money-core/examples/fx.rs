//! FX conversion: typed pairs, rounding once, and the failures that are conditions rather
//! than bugs.
//!
//! Run with `cargo run -p kamu-money-core --example fx`.

use kamu_money_core::Money;
use kamu_money_core::Rate;
use kamu_money_core::Rounding;
use kamu_money_core::StaticCurrency;
use kamu_money_core::advanced::domain::{DOMAIN_MAX, POW10_SCALE};
use kamu_money_core::errors::RateError;
use kamu_money_core::iso::{EUR, IDR, Iso4217, SGD, USD};
use std::collections::HashMap;

/// A quote table: keyed by currency CODES at runtime, but it hands out TYPED rates.
///
/// This is how "runtime SELECT, strong typing" is satisfied without a value-carrying rate
/// type in the public API. The untyped `(code, code) -> units` map is an implementation
/// detail; the only way in or out is through a generic accessor, so a caller never holds a
/// rate whose pair the compiler does not know.
///
/// In production the inner map is filled from `SELECT base, quote, units FROM fx_rates`.
#[derive(Default)]
struct QuoteTable {
    inner: HashMap<(Iso4217, Iso4217), i128>,
}

impl QuoteTable {
    fn insert<Base: StaticCurrency, Quote: StaticCurrency>(&mut self, rate: Rate<Base, Quote>) {
        self.inner.insert((Base::CODE, Quote::CODE), rate.units());
    }

    /// Runtime lookup, compile-time typed result.
    fn get<Base: StaticCurrency, Quote: StaticCurrency>(&self) -> Option<Rate<Base, Quote>> {
        self.inner.get(&(Base::CODE, Quote::CODE)).map(|&units| {
            Rate::try_from_units(units).expect("QuoteTable::insert stores only validated rates")
        })
    }
}

/// A rate expressed in whole units of quote per one base.
fn rate_of<Base: StaticCurrency, Quote: StaticCurrency>(major: i128) -> Rate<Base, Quote> {
    Rate::try_from_units(major.checked_mul(POW10_SCALE).expect("in range")).expect("in domain")
}

fn main() {
    println!("== a typed conversion ==");

    let salary = Money::<USD>::try_from_major(5_000).expect("in domain");
    let usd_idr: Rate<USD, IDR> = rate_of(16_000);

    let paid = salary.convert(usd_idr, Rounding::HalfEven).expect("well inside the domain");
    println!("  {}  at USD/IDR 16000  ->  {}", salary, paid);

    // ------------------------------------------------------------------------------------
    // WILL NOT COMPILE — the pair is checked at compile time, on BOTH ends:
    //
    //     let eur_idr: Rate<EUR, IDR> = rate_of(17_000);
    //     let _ = salary.convert(eur_idr, Rounding::HalfEven);
    //     //             ^^^^^^^ expected `Rate<USD, _>`, found `Rate<EUR, IDR>`
    //
    //     let _: Result<Money<EUR>, _> = salary.convert(usd_idr, Rounding::HalfEven);
    //     //                                    ^^^^^^^ expected `Rate<USD, EUR>`,
    //     //                                            found `Rate<USD, IDR>`
    //
    // Pinned by tests/ui/wrong_rate_pair.
    // ------------------------------------------------------------------------------------

    println!("\n== a runtime quote table that still hands out typed rates ==");

    let mut quotes = QuoteTable::default();
    quotes.insert::<USD, SGD>(rate_of(1)); // toy numbers, chosen to be exact
    quotes.insert::<SGD, IDR>(rate_of(12_000));
    quotes.insert::<USD, IDR>(rate_of(16_000));

    // Looked up by code at runtime; the result is a Rate<USD, IDR> the compiler knows.
    let found: Rate<USD, IDR> = quotes.get().expect("quote present");
    println!("  looked up USD/IDR  ->  {} units", found.units());

    // A pair nobody stored is None — distinguishable from a pair that exists, because the
    // TYPE says which pair was asked for.
    let missing: Option<Rate<IDR, EUR>> = quotes.get();
    println!("  looked up IDR/EUR  ->  {missing:?}");

    println!("\n== convert_via rounds ONCE, and it is a ledger rule, not a precision tweak ==");

    // One canonical unit, routed USD -> EUR -> IDR at 0.5 then 2.0. Sequentially the
    // intermediate is half a unit, which the ledger cannot express, so it quantises to zero
    // and the second leg multiplies nothing. Via, 0.5 * 2 == 1 and the unit survives.
    let dust = Money::<USD>::try_from_units(1).expect("in domain");
    let half: Rate<USD, EUR> = Rate::try_from_units(POW10_SCALE / 2).expect("in domain");
    let double: Rate<EUR, IDR> = rate_of(2);

    let sequential = dust
        .convert(half, Rounding::HalfEven)
        .and_then(|mid| mid.convert(double, Rounding::HalfEven))
        .expect("in domain");
    let via = dust.convert_via(half, double, Rounding::HalfEven).expect("in domain");

    println!("  sequential   {}   <- the intermediate balance ate it", sequential);
    println!("  convert_via  {}   <- one rounding, at the end", via);
    println!("  (there is no moment where a party appears to hold EUR they never held)");

    println!("\n== overflow is a CONDITION, not a bug — which is why there is no `impl Mul` ==");

    // A conversion that leaves the domain is refused by name. It reports the PAIR rather
    // than the attempted value, because the attempted value does not fit an i128 — a
    // saturated number would be a lie about what was computed.
    let vast = Money::<USD>::try_from_units(DOMAIN_MAX).expect("the domain top");
    match vast.convert(rate_of::<USD, IDR>(1_000), Rounding::HalfEven) {
        Ok(m) => println!("  unexpectedly ok: {}", m),
        Err(e @ RateError::ConversionOverflow { .. }) => println!("  refused: {e}"),
        Err(e) => println!("  refused: {e}"),
    }

    // And the dangerous near-miss: a quotient of exactly 2^128 truncates to ZERO. A
    // narrowing that used `as` would return Ok($0.00) here, with the money simply gone.
    let tricky = Money::<USD>::try_from_units(18_446_744_073_709_551_616_000_000_000).expect("ok");
    let same: Rate<USD, IDR> = Rate::try_from_units(18_446_744_073_709_551_616_000_000_000).expect("ok");
    println!("  2^128 case:  {:?}", tricky.convert(same, Rounding::HalfEven));
    println!("  ^ a truncating narrowing would have returned Ok(0) here, silently");
}
