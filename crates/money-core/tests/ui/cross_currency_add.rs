// Adding two different compile-time currencies must be a COMPILE error, not a runtime one.
// `impl Add for Money<C>` is homogeneous — `Money<USD> + Money<IDR>` has no impl to find.
// This is the headline claim of the whole Money<C> design. (DESIGN.md C3/C4)

use kamu_money_core::iso::{IDR, USD};
use kamu_money_core::money::Money;

fn main() {
    let usd = Money::<USD>::try_from_units(1).unwrap();
    let idr = Money::<IDR>::try_from_units(1).unwrap();
    let _ = usd + idr;
}
