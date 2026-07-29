// The currency pair is checked at compile time. A `Money<USD>` can only
// be converted by a `Rate<USD, _>`, and the result can only be the rate's target currency.
// Both halves are proved here — a wrong FROM end, and a target the caller mislabels.
//
// This is the claim that makes `Rate<Base, Quote>` worth having over `AnyRate`. If it ever
// holding, every typed conversion in every downstream crate would silently become unchecked,
// and no runtime test could notice.

use kamu_money_core::iso::{EUR, IDR, USD};
use kamu_money_core::Money;
use kamu_money_core::Rate;
use kamu_money_core::Rounding;

fn main() {
    let usd = Money::<USD>::try_from_units(1).unwrap();

    // The FROM end: a EUR rate cannot convert USD.
    let eur_idr = Rate::<EUR, IDR>::try_from_units(1).unwrap();
    let _ = usd.convert(eur_idr, Rounding::HalfEven);

    // The TO end: a USD->IDR rate cannot produce Money<EUR>.
    let usd_idr = Rate::<USD, IDR>::try_from_units(1).unwrap();
    let _: Result<Money<EUR>, _> = usd.convert(usd_idr, Rounding::HalfEven);
}
