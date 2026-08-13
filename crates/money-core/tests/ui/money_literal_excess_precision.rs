//! A literal carrying more fractional digits than the canonical scale must fail the BUILD.
//!
//! This is the whole claim `money!` makes. Without a compile-fail case pinning it, the macro
//! could quietly start rounding, or start accepting the literal and refusing at run time, and
//! every ordinary test would still pass — they only ever pass it amounts it accepts.

use kamu_money_core::iso::USD;
use kamu_money_core::Money;

fn main() {
    // Nineteen fractional digits; the canonical scale is eighteen.
    const OVER_SCALE: Money<USD> = kamu_money_core::money!(USD, "1.1234567890123456789");
    let _ = OVER_SCALE;
}
