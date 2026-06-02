//! Edge-case coverage: numeric-string boundaries, the `try_from_bytes` byte
//! APIs, `Category::Other`, `new_unchecked`, and serde error arms.

#![allow(missing_docs)]
#![forbid(unsafe_code)]

use kamu_iso3166::{Alpha2, Alpha3, Category, Numeric, ParseCountryError};

// --- Numeric::try_from_str boundaries -------------------------------------

#[test]
fn numeric_all_zeros_is_unassigned() {
    // Strips to a single `0`, which is in range but not an assigned code.
    assert_eq!(Numeric::try_from_str("0000"), Err(ParseCountryError::InvalidNumeric));
}

#[test]
fn numeric_long_leading_zero_run_parses() {
    // Arbitrarily many leading zeros are allowed as long as <= 3 significant
    // digits remain (360 = Indonesia).
    let n = Numeric::try_from_str("000000000360").expect("leading zeros should be stripped");
    assert_eq!(n.get(), 360);
    assert_eq!(n.to_alpha2(), Some(Alpha2::ID));
}

#[test]
fn numeric_four_significant_digits_is_out_of_range() {
    assert_eq!(Numeric::try_from_str("1000"), Err(ParseCountryError::NumericOutOfRange));
    // Even with leading zeros, 4 significant digits overflow the 3-digit range.
    assert_eq!(Numeric::try_from_str("0001000"), Err(ParseCountryError::NumericOutOfRange));
}

#[test]
fn numeric_in_range_but_unassigned() {
    assert_eq!(Numeric::try_from_str("999"), Err(ParseCountryError::InvalidNumeric));
}

#[test]
fn numeric_empty_and_non_digit_inputs() {
    assert_eq!(Numeric::try_from_str(""), Err(ParseCountryError::NotAnInteger));
    assert_eq!(Numeric::try_from_str("12x"), Err(ParseCountryError::NotAnInteger));
    // Non-ASCII bytes are reported distinctly from a plain non-integer.
    assert_eq!(Numeric::try_from_str("1é"), Err(ParseCountryError::NonAscii));
}

// --- new_unchecked ---------------------------------------------------------

#[test]
fn new_unchecked_skips_validation() {
    // 999 is unassigned, but new_unchecked constructs it anyway.
    let n = Numeric::new_unchecked(999);
    assert_eq!(n.get(), 999);
    assert_eq!(n.to_alpha2(), None);
}

// --- Alpha2 / Alpha3 try_from_bytes ---------------------------------------

#[test]
fn alpha2_from_bytes_is_case_insensitive() {
    assert_eq!(Alpha2::try_from_bytes(b"id"), Ok(Alpha2::ID));
}

#[test]
fn alpha2_from_bytes_error_paths() {
    // Wrong length.
    assert!(matches!(
        Alpha2::try_from_bytes(b"abc"),
        Err(ParseCountryError::InvalidLength { expected: 2, got: 3 })
    ));
    // Exactly two bytes, but non-ASCII (UTF-8 'é').
    assert_eq!(Alpha2::try_from_bytes(&[0xC3, 0xA9]), Err(ParseCountryError::NonAscii));
    // Right shape, unknown code.
    assert_eq!(Alpha2::try_from_bytes(b"zz"), Err(ParseCountryError::InvalidAlpha2));
}

#[test]
fn alpha3_from_bytes_error_paths() {
    assert_eq!(Alpha3::try_from_bytes(b"idn"), Ok(Alpha3::IDN));
    assert!(matches!(
        Alpha3::try_from_bytes(b"ab"),
        Err(ParseCountryError::InvalidLength { expected: 3, got: 2 })
    ));
    // Three bytes, non-ASCII in the middle.
    assert_eq!(Alpha3::try_from_bytes(&[b'I', 0xC3, 0xA9]), Err(ParseCountryError::NonAscii));
    assert_eq!(Alpha3::try_from_bytes(b"zzz"), Err(ParseCountryError::InvalidAlpha3));
}

// --- Category::Other -------------------------------------------------------

#[test]
fn category_other_round_trips_through_as_str() {
    let cat = Category::Other("FUTURE-CATEGORY");
    assert_eq!(cat.as_str(), "FUTURE-CATEGORY");
}

// --- serde error arms ------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_arms {
    use super::{Category, Numeric};

    #[test]
    fn numeric_from_unsigned_integer() {
        let n: Numeric = serde_json::from_str("360").expect("u64 path");
        assert_eq!(n.get(), 360);
    }

    #[test]
    fn numeric_unsigned_out_of_u16_range_errors() {
        // Exercises the visit_u64 try_into failure arm.
        assert!(serde_json::from_str::<Numeric>("70000").is_err());
    }

    #[test]
    fn numeric_in_range_but_unassigned_errors() {
        // Exercises the visit_u64 try_from_u16 failure arm.
        assert!(serde_json::from_str::<Numeric>("999").is_err());
    }

    #[test]
    fn numeric_negative_integer_errors() {
        // Exercises the visit_i64 arm.
        assert!(serde_json::from_str::<Numeric>("-1").is_err());
    }

    #[test]
    fn numeric_from_string() {
        // Exercises the visit_str arm.
        let n: Numeric = serde_json::from_str("\"360\"").expect("str path");
        assert_eq!(n.get(), 360);
    }

    #[test]
    fn category_other_serializes_as_its_raw_string() {
        let json = serde_json::to_string(&Category::Other("CUSTOM")).expect("serialize");
        assert_eq!(json, "\"CUSTOM\"");
    }

    #[test]
    fn category_unknown_string_fails_to_deserialize() {
        // Exercises the visit_str error arm (Other cannot be deserialized).
        assert!(serde_json::from_str::<Category>("\"NOT-A-CATEGORY\"").is_err());
    }

    #[test]
    fn category_wrong_json_type_reports_expectation() {
        // A non-string token makes serde format the visitor's `expecting`.
        assert!(serde_json::from_str::<Category>("123").is_err());
    }
}
