//! Tests for the 0.2.0 API additions and the deprecated back-compat aliases.

#![allow(missing_docs, deprecated)]
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

#[test]
fn deprecated_module_aliases_still_resolve() {
    // Pre-0.2.0 paths remain usable (deprecation warnings silenced above).
    let a2: kamu_iso3166::one::Alpha2 = Alpha2::ID;
    let a3: kamu_iso3166::one::Alpha3 = Alpha3::IDN;
    let n: kamu_iso3166::one::Numeric = a2.to_numeric();
    assert_eq!(a2.to_alpha3(), a3);
    assert_eq!(n.get(), 360);

    let sd: &kamu_iso3166::two::Subdivision = kamu_iso3166::two::Subdivision::try_from_str("ID-JK").unwrap();
    let cat: kamu_iso3166::two::Category = sd.category;
    assert_eq!(sd.parent, Alpha2::ID);
    assert!(!cat.as_str().is_empty());
}
