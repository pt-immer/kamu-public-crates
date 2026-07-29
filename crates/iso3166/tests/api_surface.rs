//! Tests for the stable public API surface.

#![allow(missing_docs)]
#![forbid(unsafe_code)]

use kamu_iso3166::{Alpha2, Alpha3, Numeric, Subdivision};

#[test]
fn iter_matches_all_slice() {
    assert!(Alpha2::iter().eq(Alpha2::ALL.iter().copied()));
    assert!(Alpha3::iter().eq(Alpha3::ALL.iter().copied()));
    assert!(Subdivision::iter().eq(kamu_iso3166::subdivision::ALL_SUBDIVISIONS.iter()));
}

#[test]
fn numeric_new_validates_like_try_from_u16() {
    // const-context construction works
    const ID: Option<Numeric> = Numeric::new(360);
    assert!(ID.is_some());

    assert_eq!(Numeric::new(360), Some(Alpha2::ID.to_numeric()));
    assert_eq!(Numeric::new(360).map(Numeric::get), Some(360));
    assert_eq!(Numeric::new(1000), None); // out of range
    assert_eq!(Numeric::new(999), None); // in range but unassigned
}
