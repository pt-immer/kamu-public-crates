//! Exhaustive coverage of every conversion trait impl and every documented
//! error variant for the public types.

#![allow(missing_docs)]
#![forbid(unsafe_code)]

use core::str::FromStr;

use kamu_iso3166::{Alpha2, Alpha3, Numeric, ParseCountryError, ParseSubdivisionError, Subdivision};

#[test]
fn alpha2_traits_and_errors() {
    let id = Alpha2::ID;
    assert_eq!(Alpha2::from_str("id").unwrap(), id);
    assert_eq!(<Alpha2 as TryFrom<&str>>::try_from("ID").unwrap(), id);
    assert_eq!(Alpha3::from(id), Alpha3::IDN);
    assert_eq!(Numeric::from(id), id.to_numeric());
    assert_eq!(u16::from(id), 360);
    assert_eq!(<Alpha2 as AsRef<str>>::as_ref(&id), "ID");

    assert_eq!(Alpha2::try_from_bytes(b"X"), Err(ParseCountryError::InvalidLength { expected: 2, got: 1 }),);
    assert_eq!(Alpha2::try_from_bytes(&[0xFF, 0xFE]), Err(ParseCountryError::NonAscii));
    assert_eq!(Alpha2::try_from_str("ZZ"), Err(ParseCountryError::InvalidAlpha2));
}

#[test]
fn alpha3_traits_and_errors() {
    let idn = Alpha3::IDN;
    assert_eq!(Alpha3::from_str("idn").unwrap(), idn);
    assert_eq!(<Alpha3 as TryFrom<&str>>::try_from("IDN").unwrap(), idn);
    assert_eq!(Alpha2::from(idn), Alpha2::ID);
    assert_eq!(Numeric::from(idn), idn.to_numeric());
    assert_eq!(u16::from(idn), 360);
    assert_eq!(<Alpha3 as AsRef<str>>::as_ref(&idn), "IDN");

    assert_eq!(Alpha3::try_from_bytes(b"XY"), Err(ParseCountryError::InvalidLength { expected: 3, got: 2 }),);
    assert_eq!(Alpha3::try_from_bytes(&[0xFF, 0xFE, 0xFD]), Err(ParseCountryError::NonAscii));
    assert_eq!(Alpha3::try_from_str("ZZZ"), Err(ParseCountryError::InvalidAlpha3));
}

#[test]
fn numeric_traits_and_errors() {
    let n = Numeric::try_from(360u16).unwrap();
    assert_eq!(Numeric::from_str("360").unwrap(), n);
    assert_eq!(<Numeric as TryFrom<&str>>::try_from("0360").unwrap(), n);
    assert_eq!(u16::from(n), 360);
    assert_eq!(Alpha2::try_from(n).unwrap(), Alpha2::ID);
    assert_eq!(Alpha3::try_from(n).unwrap(), Alpha3::IDN);

    assert_eq!(Numeric::try_from(9999u16), Err(ParseCountryError::NumericOutOfRange));
    assert_eq!(Numeric::try_from(999u16), Err(ParseCountryError::InvalidNumeric));
    assert_eq!(Numeric::try_from_str(""), Err(ParseCountryError::NotAnInteger));
    assert_eq!(Numeric::try_from_str("abc"), Err(ParseCountryError::NotAnInteger));
    assert_eq!(Numeric::try_from_str("12.3"), Err(ParseCountryError::NotAnInteger));
    // Multi-byte non-ASCII digits are reported as NonAscii, not NotAnInteger.
    assert_eq!(Numeric::try_from_str("\u{0663}\u{0666}\u{0660}"), Err(ParseCountryError::NonAscii));
}

#[test]
fn unassigned_numeric_conversions_fail_gracefully() {
    // `new_unchecked` is the only way to obtain an unassigned Numeric, which
    // lets us exercise the `None` / `InvalidNumeric` branches.
    let bogus = Numeric::new_unchecked(1);
    assert_eq!(bogus.get(), 1);
    assert_eq!(bogus.to_alpha2(), None);
    assert_eq!(bogus.to_alpha3(), None);
    assert_eq!(Alpha2::try_from(bogus), Err(ParseCountryError::InvalidNumeric));
    assert_eq!(Alpha3::try_from(bogus), Err(ParseCountryError::InvalidNumeric));
    assert_eq!(Numeric::new(1), None);
    assert_eq!(Numeric::try_from_u16(1), Err(ParseCountryError::InvalidNumeric));
}

#[test]
fn subdivision_traits_and_errors() {
    let jk = Subdivision::try_from_str("ID-JK").unwrap();

    // FromStr / TryFrom<&str> return owned copies of the static entry.
    let owned: Subdivision = Subdivision::from_str("id-jk").unwrap();
    assert_eq!(owned, *jk);
    let owned2: Subdivision = <Subdivision as TryFrom<&str>>::try_from("ID-JK").unwrap();
    assert_eq!(owned2, *jk);
    assert_eq!(<Subdivision as AsRef<str>>::as_ref(&owned), "ID-JK");

    assert_eq!(Subdivision::try_from_str("ID").unwrap_err(), ParseSubdivisionError::InvalidLength { got: 2 },);
    assert_eq!(Subdivision::try_from_str("IDJKK").unwrap_err(), ParseSubdivisionError::MissingSeparator);
    assert_eq!(Subdivision::try_from_str("ZZ-JK").unwrap_err(), ParseSubdivisionError::InvalidParent);
    assert_eq!(Subdivision::try_from_str("ID-ZZ").unwrap_err(), ParseSubdivisionError::UnknownSubdivision);
    // "ID-\u{00e9}" is 5 bytes, in-range length but non-ASCII.
    assert_eq!(Subdivision::try_from_str("ID-\u{00e9}").unwrap_err(), ParseSubdivisionError::NonAscii);
}
