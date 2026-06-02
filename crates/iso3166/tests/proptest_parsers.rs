//! Property-based fuzzing for every public parser: valid, invalid,
//! case-insensitivity, non-ASCII and length-boundary inputs. No parser may
//! ever panic, and every documented error path must be reachable.

#![allow(missing_docs, clippy::missing_panics_doc)]
#![forbid(unsafe_code)]

use kamu_iso3166::subdivision::{ALL_SUBDIVISIONS, SUBDIVISION_COUNT};
use kamu_iso3166::{Alpha2, Alpha3, Numeric, ParseCountryError, Subdivision};
use proptest::prelude::*;

/// Flip the ASCII case of `s` per bit of `mask` (bit i set => lowercase char i).
fn flip_case(s: &str, mask: u64) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| if (mask >> (i % 64)) & 1 == 1 { c.to_ascii_lowercase() } else { c })
        .collect()
}

proptest! {
    // --- never panic on arbitrary (possibly non-UTF8-shaped) input ---
    #[test]
    fn alpha2_never_panics(s in ".*") { let _ = Alpha2::try_from_str(&s); }

    #[test]
    fn alpha3_never_panics(s in ".*") { let _ = Alpha3::try_from_str(&s); }

    #[test]
    fn numeric_never_panics(s in ".*") { let _ = Numeric::try_from_str(&s); }

    #[test]
    fn subdivision_never_panics(s in ".*") { let _ = Subdivision::try_from_str(&s); }

    // --- length errors for the fixed-width country codes ---
    #[test]
    fn alpha2_wrong_length_is_length_error(s in "[A-Za-z]{0,8}") {
        prop_assume!(s.len() != 2);
        prop_assert_eq!(
            Alpha2::try_from_str(&s),
            Err(ParseCountryError::InvalidLength { expected: 2, got: s.len() }),
        );
    }

    #[test]
    fn alpha3_wrong_length_is_length_error(s in "[A-Za-z]{0,8}") {
        prop_assume!(s.len() != 3);
        prop_assert_eq!(
            Alpha3::try_from_str(&s),
            Err(ParseCountryError::InvalidLength { expected: 3, got: s.len() }),
        );
    }

    // --- case-insensitive acceptance, canonical uppercase identity ---
    #[test]
    fn alpha2_is_case_insensitive(idx in 0usize..Alpha2::COUNT, mask in any::<u64>()) {
        let a2 = Alpha2::ALL[idx];
        prop_assert_eq!(Alpha2::try_from_str(&flip_case(a2.as_str(), mask)), Ok(a2));
    }

    #[test]
    fn alpha3_is_case_insensitive(idx in 0usize..Alpha3::COUNT, mask in any::<u64>()) {
        let a3 = Alpha3::ALL[idx];
        prop_assert_eq!(Alpha3::try_from_str(&flip_case(a3.as_str(), mask)), Ok(a3));
    }

    // --- two non-ASCII bytes are length-2 but rejected as NonAscii ---
    #[test]
    fn alpha2_two_nonascii_bytes(hi in 0x80u8..=0xBF, lo in 0x80u8..=0xBF) {
        prop_assert_eq!(Alpha2::try_from_bytes(&[hi, lo]), Err(ParseCountryError::NonAscii));
    }

    // --- numeric: assigned codes round-trip with arbitrary leading zeros ---
    #[test]
    fn numeric_assigned_roundtrip_with_padding(idx in 0usize..Alpha2::COUNT, pad in 0u32..6) {
        let n = Alpha2::ALL[idx].to_numeric();
        let s = format!("{:0width$}", n.get(), width = (3 + pad) as usize);
        prop_assert_eq!(Numeric::try_from_str(&s), Ok(n));
    }

    #[test]
    fn numeric_display_then_parse_is_identity(idx in 0usize..Alpha2::COUNT) {
        let n = Alpha2::ALL[idx].to_numeric();
        prop_assert_eq!(n.to_string().parse::<Numeric>(), Ok(n));
    }

    #[test]
    fn numeric_above_999_is_out_of_range(v in 1000u32..1_000_000) {
        prop_assert_eq!(
            Numeric::try_from_str(&v.to_string()),
            Err(ParseCountryError::NumericOutOfRange),
        );
    }

    #[test]
    fn numeric_new_matches_try_from_u16(v in any::<u16>()) {
        prop_assert_eq!(Numeric::new(v), Numeric::try_from_u16(v).ok());
    }

    #[test]
    fn numeric_non_digit_ascii_is_not_an_integer(s in "[A-Za-z ._/-]{1,6}") {
        prop_assert_eq!(Numeric::try_from_str(&s), Err(ParseCountryError::NotAnInteger));
    }

    // --- subdivisions: real codes round-trip under arbitrary case ---
    #[test]
    fn subdivision_is_case_insensitive(idx in 0usize..SUBDIVISION_COUNT, mask in any::<u64>()) {
        let sd = &ALL_SUBDIVISIONS[idx];
        prop_assert_eq!(
            Subdivision::try_from_str(&flip_case(sd.code, mask)).map(|x| x.code),
            Ok(sd.code),
        );
    }
}
