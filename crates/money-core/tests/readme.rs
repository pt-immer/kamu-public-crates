//! The README's Rust examples, compiled.
//!
//! A README that names a method the crate does not have is a bug that no other test can fail
//! on — it is the first thing a reader tries and the last thing a test suite covers. These
//! mirror the examples in `README.md`; if you change one, change the other.
//!
//! (The serde example there is not here: it needs the `serde` feature and derive macros, and
//! `src/wire/`'s own tests already cover the same shapes.)

use core::num::NonZeroU32;
use kamu_money_core::{Money, Rounding, iso::USD};

/// The opening example: allocation conserves the total exactly.
#[test]
fn allocation_conserves_the_total() {
    let whole = Money::<USD>::try_from_major(10).unwrap();
    let parts = whole.allocate(&[1, 1, 1]).unwrap();
    assert_eq!(parts.iter().map(|p| p.units()).sum::<i128>(), whole.units());
}

/// The division example: both exits from a `Division`, spelled as the README spells them.
#[test]
fn a_division_has_exactly_two_exits() {
    let whole = Money::<USD>::try_from_major(10).unwrap();
    let three = NonZeroU32::new(3).unwrap();

    let (each, residue) = whole.div_int(three, Rounding::TowardZero).take_residue();
    assert_eq!(residue.take_units(), 1, "10 into 3 leaves 1 unit at scale 18");

    let same = whole.div_int(three, Rounding::TowardZero).discard_deliberately();
    assert_eq!(each, same, "the two exits agree on the quotient");
}
