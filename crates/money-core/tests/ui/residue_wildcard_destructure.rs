// The residue cannot be thrown away by a wildcard destructure, because there is no tuple to
// destructure. `div_int` returns ONE value bundling the quotient and the residue, so reaching
// the money requires calling something that also decides the residue's fate.
//
// This case exists because the pattern it forbids is the one a developer actually reaches for,
// AND the one rustc recommends: `#[must_use]` does not survive `let (share, _) = ...`, and the
// compiler suggests the `_` prefix that defeats it. The bundled `Division` makes it fail to
// compile.

use core::num::NonZeroU32;
use kamu_money_core::iso::USD;
use kamu_money_core::Money;
use kamu_money_core::Rounding;

fn main() {
    let m = Money::<USD>::try_from_units(10_000_000_000_000_000_000).unwrap();
    // 10.000000000000000000 / 3 leaves one unit over.
    let (_share, _) = m.div_int(NonZeroU32::new(3).unwrap(), Rounding::TowardZero);
}
