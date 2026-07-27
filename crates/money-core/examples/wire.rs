//! The serde wire: two modes per field, and the trap that makes the codec hand-written.
//!
//! Run with `cargo run -p kamu-money-core --example wire --features serde`.

use kamu_money_core::POW10_SCALE;
use kamu_money_core::iso::{IDR, Iso4217, JPY, USD};
use kamu_money_core::money::Money;
use kamu_money_core::rate::Rate;
use serde::{Deserialize, Serialize};

/// A payload mixing both modes. Which one a field uses is chosen **per field**, at compile
/// time, and a typo in the `with` path is `E0433` at build time rather than a runtime surprise.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Invoice {
    /// The default. An object, so a consumer reads `.currency` without parsing anything.
    total: Money<IDR>,

    /// One scalar. Compact, and the shape most existing APIs already use.
    #[serde(with = "kamu_money_core::wire::transparent")]
    tax: Money<IDR>,

    /// Rates work the same way, in ISO 15022 field 92B's `BASE/QUOTE/RATE` shape.
    #[serde(with = "kamu_money_core::wire::transparent")]
    booked_at: Rate<USD, IDR>,
}

fn main() {
    let invoice = Invoice {
        total: Money::<IDR>::from_major(1_600_000).expect("in domain"),
        tax: Money::<IDR>::from_units(176_000_500_000_000_000_000_000).expect("in domain"),
        booked_at: Rate::<USD, IDR>::from_units(16_000 * POW10_SCALE).expect("in domain"),
    };

    println!("== JSON: structured by default, transparent where asked ==");
    let json = serde_json::to_string_pretty(&invoice).expect("serialises");
    println!("{json}");

    let back: Invoice = serde_json::from_str(&json).expect("round-trips");
    assert_eq!(back, invoice);
    println!("  round-trip: OK");

    println!("\n== the amount uses the same trim rule as Display ==");
    let units = 10_500_000_000_000_000_000; // 10.5, whatever the currency
    println!(
        "  USD (settles 2dp)   {}",
        serde_json::to_string(&Money::<USD>::from_units(units).unwrap()).expect("ok")
    );
    println!(
        "  JPY (settles 0dp)   {}",
        serde_json::to_string(&Money::<JPY>::from_units(units).unwrap()).expect("ok")
    );
    println!("  ^ one rule, one implementation — Display and the wire cannot disagree");

    println!("\n== the currency in the payload is a CROSS-CHECK, not decoration ==");
    match serde_json::from_str::<Money<IDR>>(r#"{"currency":"USD","amount":"10.50"}"#) {
        Ok(m) => println!("  unexpectedly accepted: {m}"),
        Err(e) => println!("  USD payload into a Money<IDR> field -> {e}"),
    }
    println!("  ^ catches a currency landing in the wrong field, where types cannot help");

    println!("\n== excess precision is REFUSED, never rounded ==");
    match serde_json::from_str::<Money<USD>>(r#"{"currency":"USD","amount":"0.0000000000000000005"}"#) {
        Ok(m) => println!("  unexpectedly accepted: {m}"),
        Err(e) => println!("  19 decimal places -> {e}"),
    }
    println!("  ^ rust_decimal's from_str returned Ok here and rounded silently (specs.md E2)");

    println!("\n== binary: the ISO NUMERIC, never the variant's position ==");
    println!("  postcard(Iso4217::IDR)  = {:?}", postcard::to_allocvec(&Iso4217::IDR).expect("ok"));
    println!(
        "  postcard(360u16)        = {:?}   <- the ISO numeric code",
        postcard::to_allocvec(&360u16).expect("ok")
    );
    println!(
        "  postcard(1u16)          = {:?}   <- IDR's ORDINAL POSITION in the table",
        postcard::to_allocvec(&1u16).expect("ok")
    );
    println!();
    println!("  A derived impl would emit the position. Insert one currency mid-table and every");
    println!("  later code shifts: stored IDR decodes as GBP, silently, with #[repr(u16)] and");
    println!("  IDR = 360 unchanged in BOTH versions. The register is complete at 178 and still");
    println!("  grows as ISO issues codes; variants are alpha-3 ordered, so a new code lands");
    println!("  BETWEEN existing ones and shifts every later ordinal.");
    println!();
    println!("  A JSON suite CANNOT catch it — human-readable formats emit the NAME.");

    println!("\n== binary tags the currency, so a wrong type is REFUSED (R2-F2) ==");
    let money = postcard::to_allocvec(&Money::<USD>::from_units(units).unwrap()).expect("ok");
    let bare = postcard::to_allocvec(&units).expect("ok");
    println!("  postcard(Money<USD>) = {} bytes", money.len());
    println!(
        "  postcard(i128)       = {} bytes  <- SHORTER: the two ISO-numeric bytes are the tag",
        bare.len()
    );
    assert_ne!(money, bare, "the currency is on the wire now");
    // The point of the tag: the same bytes must not decode as a different currency.
    let as_idr = postcard::from_bytes::<Money<IDR>>(&money);
    println!(
        "  decode USD bytes as Money<IDR> -> {}",
        if as_idr.is_err() { "Err (refused)" } else { "Ok (BUG)" }
    );
    assert!(as_idr.is_err(), "a bare i128 would have silently reinterpreted this");
}
