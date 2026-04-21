#![allow(missing_docs, clippy::assertions_on_constants)]
#![forbid(unsafe_code)]

use kamu_iso3166::{Alpha2, Alpha3, Category, Numeric, Subdivision};

#[test]
fn counts_are_expected() {
    assert_eq!(Alpha2::ALL.len(), Alpha2::COUNT);
    assert_eq!(Alpha3::ALL.len(), Alpha3::COUNT);
    assert_eq!(Alpha2::COUNT, Alpha3::COUNT);
    assert!(Alpha2::COUNT >= 240);
}

#[test]
fn alpha2_indonesia_basics() {
    let id = Alpha2::ID;
    assert_eq!(id.as_str(), "ID");
    assert_eq!(id.short_name(), "Indonesia");
    assert_eq!(id.to_alpha3(), Alpha3::IDN);
    assert_eq!(id.to_numeric(), Numeric::try_from(360u16).unwrap());
    assert_eq!(id.to_string(), "ID");
}

#[test]
fn numeric_zero_pads_display() {
    let n = Numeric::try_from(4u16).unwrap();
    assert_eq!(n.to_string(), "004");
    assert_eq!(n.to_alpha2(), Some(Alpha2::AF));
}

#[test]
fn round_trip_all_countries() {
    for &a2 in Alpha2::ALL {
        let a3 = a2.to_alpha3();
        let n = a2.to_numeric();
        assert_eq!(a3.to_alpha2(), a2, "alpha3->alpha2: {a3:?}");
        assert_eq!(a3.to_numeric(), n, "alpha3.to_numeric mismatch for {a2:?}");
        assert_eq!(n.to_alpha2(), Some(a2), "numeric->alpha2 for {a2:?}");
        assert_eq!(n.to_alpha3(), Some(a3), "numeric->alpha3 for {a2:?}");

        let s = a2.as_str();
        assert_eq!(Alpha2::try_from_str(s).unwrap(), a2);
        assert_eq!(Alpha2::try_from_str(&s.to_ascii_lowercase()).unwrap(), a2);

        let s3 = a3.as_str();
        assert_eq!(Alpha3::try_from_str(s3).unwrap(), a3);
        assert_eq!(Alpha3::try_from_str(&s3.to_ascii_lowercase()).unwrap(), a3);
    }
}

#[test]
fn parsing_errors() {
    use kamu_iso3166::ParseCountryError::*;
    assert_eq!(Alpha2::try_from_str("X").unwrap_err(), InvalidLength { expected: 2, got: 1 });
    assert_eq!(Alpha2::try_from_str("XY").unwrap_err(), InvalidAlpha2);
    assert_eq!(Alpha3::try_from_str("XYZ").unwrap_err(), InvalidAlpha3);
    assert_eq!(Numeric::try_from_str("abc").unwrap_err(), NotAnInteger);
    assert_eq!(Numeric::try_from_str("9999").unwrap_err(), NumericOutOfRange);
    assert_eq!(Numeric::try_from_str("999").unwrap_err(), InvalidNumeric);
    assert_eq!(Numeric::try_from_str("0360").unwrap().get(), 360);
    assert_eq!(Numeric::try_from_str("00360").unwrap().get(), 360);
}

#[test]
fn subdivisions_indonesia() {
    let subs = Alpha2::ID.subdivisions();
    assert!(!subs.is_empty(), "Indonesia should have subdivisions");
    for s in subs {
        assert_eq!(s.parent, Alpha2::ID);
        assert!(s.code.starts_with("ID-"));
    }
    let jk: &Subdivision = Subdivision::try_from_str("ID-JK").unwrap();
    assert_eq!(jk.parent, Alpha2::ID);
    assert_eq!(jk.code, "ID-JK");
    assert_eq!(Subdivision::try_from_str("id-jk").unwrap().code, "ID-JK");
    let _ = jk.category; // just ensure it's readable
    let _ = Category::Other("unused"); // ensure fallback variant exists
}

#[test]
fn subdivision_parse_errors() {
    use kamu_iso3166::ParseSubdivisionError::*;
    assert!(matches!(Subdivision::try_from_str("ID"), Err(InvalidLength { got: 2 })));
    assert!(matches!(Subdivision::try_from_str("IDJK"), Err(MissingSeparator)));
    assert!(matches!(Subdivision::try_from_str("ZZ-JK"), Err(InvalidParent)));
    assert!(matches!(Subdivision::try_from_str("ID-ZZ"), Err(UnknownSubdivision)));
}
