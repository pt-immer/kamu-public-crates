//! The pre-`const` parser, kept as a reference, and differentially compared.
//!
//! [`parse_fixed_point`] was rewritten from iterators and `?` into indexed loops so that it
//! could be `const`. A rewrite of the crate's only parser is exactly the change that can
//! pass every hand-written case and still have moved a boundary: which of two errors is
//! reported first, whether a trailing point is still accepted, where a second `.` is caught.
//!
//! So the old body is preserved verbatim below and the two are compared on every string
//! over a money-shaped alphabet up to length five — 19,608 inputs, exhaustive rather than
//! sampled — plus a property run over long and large-magnitude inputs that short strings
//! cannot reach. Agreement is on the **full** `Result`, so a changed error variant fails
//! just as loudly as a changed value.

use super::fixed_point::parse_fixed_point;
use crate::domain::SCALE;
use crate::errors::ParseMoneyError;
use crate::text::SCALE_USIZE;
use proptest::prelude::*;

/// The parser exactly as it stood before the `const` rewrite.
///
/// Not tidied, not modernised. Its value is being the previous behaviour, so any edit to it
/// destroys the only thing it is for.
#[allow(clippy::arithmetic_side_effects, clippy::as_conversions)]
fn legacy_parse_fixed_point(text: &str) -> Result<i128, ParseMoneyError> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };

    let (whole_text, frac_text) = match digits.split_once('.') {
        Some((w, fr)) => (w, fr),
        None => (digits, ""),
    };

    if whole_text.is_empty() || !whole_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseMoneyError::InvalidSyntax);
    }
    if !frac_text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseMoneyError::InvalidSyntax);
    }

    let supplied = u32::try_from(frac_text.len()).map_err(|_| ParseMoneyError::InvalidSyntax)?;
    if supplied > SCALE {
        return Err(ParseMoneyError::ExcessPrecision { digits: supplied });
    }

    let mut magnitude: u128 = 0;
    for byte in
        whole_text.bytes().chain(frac_text.bytes().chain(core::iter::repeat(b'0')).take(SCALE as usize))
    {
        let digit = u128::from(
            byte.checked_sub(b'0').expect("bytes were verified as ASCII digits, or are padding zeros"),
        );
        magnitude =
            magnitude.checked_mul(10).and_then(|shifted| shifted.checked_add(digit)).ok_or(if negative {
                ParseMoneyError::NegativeMagnitudeOverflow
            } else {
                ParseMoneyError::PositiveMagnitudeOverflow
            })?;
    }

    if negative && magnitude == i128::MIN.unsigned_abs() {
        return Ok(i128::MIN);
    }

    let units = i128::try_from(magnitude).map_err(|_| {
        if negative {
            ParseMoneyError::NegativeMagnitudeOverflow
        } else {
            ParseMoneyError::PositiveMagnitudeOverflow
        }
    })?;
    if negative {
        Ok(units.checked_neg().expect("positive i128 magnitude can always be negated"))
    } else {
        Ok(units)
    }
}

/// Every structural shape a short input can take: both signs, the point in every position,
/// missing parts, repeated separators, and a non-digit to force the syntax arm.
const ALPHABET: [u8; 7] = [b'0', b'1', b'9', b'-', b'.', b'a', b' '];

#[test]
fn the_rewrite_agrees_with_the_previous_parser_on_every_short_input() {
    let mut checked = 0_u32;
    for length in 0..=5_usize {
        let count = ALPHABET.len().pow(u32::try_from(length).expect("length is at most five"));
        for mut code in 0..count {
            let mut text = String::with_capacity(length);
            for _ in 0..length {
                text.push(char::from(ALPHABET[code % ALPHABET.len()]));
                code /= ALPHABET.len();
            }
            assert_eq!(
                parse_fixed_point(&text),
                legacy_parse_fixed_point(&text),
                "the const rewrite changed the verdict for {text:?}",
            );
            checked = checked.checked_add(1).expect("the corpus is far below u32::MAX");
        }
    }
    // A positive control on the loop itself: a silent off-by-one in the odometer would
    // otherwise let this test pass having compared almost nothing.
    assert_eq!(checked, 19_608, "the exhaustive corpus did not have the size it claims");
}

/// Every fraction width from none to well past the scale, both signs.
///
/// The exhaustive pass above stops at five characters, so it cannot reach an eighteen-digit
/// fraction — the boundary `supplied > SCALE` decides. Leaving that to the property run
/// would leave it to chance: a `>` weakened to `>=` changes the verdict for exactly one
/// width out of the forty-two swept here.
#[test]
fn the_rewrite_agrees_at_every_fraction_width_around_the_scale() {
    for sign in ["", "-"] {
        for width in 0..=20_usize {
            let text = format!("{sign}1.{}", "7".repeat(width));
            assert_eq!(
                parse_fixed_point(&text),
                legacy_parse_fixed_point(&text),
                "the const rewrite changed the verdict at fraction width {width}",
            );
        }
    }
    // The boundary is where it is claimed to be, in case both parsers were rewritten
    // together and agreed on the wrong answer.
    let at_scale = format!("1.{}", "7".repeat(SCALE_USIZE));
    let past_scale = format!("1.{}", "7".repeat(SCALE_USIZE + 1));
    assert!(parse_fixed_point(&at_scale).is_ok(), "the scale itself must parse");
    assert_eq!(parse_fixed_point(&past_scale), Err(ParseMoneyError::ExcessPrecision { digits: SCALE + 1 }),);
}

/// The exact `i128` limits, which no generated corpus reaches by chance.
///
/// Two branches turn on a single value each — the `i128::MIN` special case, and the
/// `magnitude > i128::MAX` narrowing guard — and both need a 39-digit magnitude with the
/// point eighteen places from the right. Mutation-testing the harness found that weakening
/// either comparison to `>=`, or deleting the `i128::MIN` arm outright, survived every
/// other test in this module. These are the literals that kill those mutants.
///
/// `parse_amount` rejects all four as out of domain, which is exactly why they belong
/// *here*: this is the layer where the difference is observable.
#[test]
fn the_rewrite_agrees_on_the_exact_i128_limits() {
    for text in [
        // i128::MAX, and one canonical unit either side of it.
        "170141183460469231731.687303715884105727",
        "170141183460469231731.687303715884105726",
        "170141183460469231731.687303715884105728",
        // i128::MIN, whose magnitude has no positive counterpart, and its neighbours.
        "-170141183460469231731.687303715884105728",
        "-170141183460469231731.687303715884105727",
        "-170141183460469231731.687303715884105729",
    ] {
        assert_eq!(
            parse_fixed_point(text),
            legacy_parse_fixed_point(text),
            "the const rewrite changed the verdict at an i128 limit: {text:?}",
        );
    }
    // Pinned absolutely as well, so both parsers cannot drift here together.
    assert_eq!(parse_fixed_point("170141183460469231731.687303715884105727"), Ok(i128::MAX));
    assert_eq!(parse_fixed_point("-170141183460469231731.687303715884105728"), Ok(i128::MIN));
    assert_eq!(
        parse_fixed_point("170141183460469231731.687303715884105728"),
        Err(ParseMoneyError::PositiveMagnitudeOverflow),
    );
    assert_eq!(
        parse_fixed_point("-170141183460469231731.687303715884105729"),
        Err(ParseMoneyError::NegativeMagnitudeOverflow),
    );
}

/// Magnitudes stepping across the accumulator's limits, which short inputs cannot reach.
#[test]
fn the_rewrite_agrees_where_the_accumulator_runs_out() {
    for digits in 1..=45_usize {
        for sign in ["", "-"] {
            let text = format!("{sign}{}", "9".repeat(digits));
            assert_eq!(
                parse_fixed_point(&text),
                legacy_parse_fixed_point(&text),
                "the const rewrite changed the verdict at {digits} whole digits",
            );
        }
    }
}

proptest! {
    /// Long inputs, which the exhaustive pass cannot reach: fractions at and past the
    /// scale, and magnitudes that exhaust the accumulator.
    #[test]
    fn the_rewrite_agrees_on_long_and_large_inputs(
        text in r"-?[0-9]{0,40}\.?[0-9]{0,40}",
    ) {
        prop_assert_eq!(parse_fixed_point(&text), legacy_parse_fixed_point(&text));
    }

    /// Arbitrary bytes, so the syntax arms are compared on input neither parser was
    /// designed around, including multi-byte UTF-8.
    #[test]
    fn the_rewrite_agrees_on_arbitrary_text(text in ".{0,24}") {
        prop_assert_eq!(parse_fixed_point(&text), legacy_parse_fixed_point(&text));
    }
}
