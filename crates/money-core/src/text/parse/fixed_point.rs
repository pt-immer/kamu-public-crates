//! The const decimal parser and its primitives.

use super::super::SCALE_USIZE;
use crate::domain::SCALE;
use crate::errors::ParseMoneyError;

/// The magnitude of [`i128::MAX`], as the unsigned accumulator holds it.
///
/// The bound `i128::try_from` would check, written so that a const fn can check it too.
const I128_MAX_MAGNITUDE: u128 = i128::MAX.unsigned_abs();
/// One past a loop index already known to sit inside a slice.
///
/// `index + 1` is denied by `clippy::arithmetic_side_effects`, which cannot see that bound.
/// The `expect` is the bound, said out loud.
const fn next(index: usize) -> usize {
    index.checked_add(1).expect("an index inside a slice that exists cannot reach usize::MAX")
}

/// The value of an ASCII digit, widened for accumulation. `None` for anything else.
///
/// A match rather than `byte - b'0'` widened by a cast: `u128::from` is not const, and
/// `clippy::as_conversions` is denied crate-wide. Being **total** is the other half of the
/// bargain — the byte-subtraction form needed an `expect` to restate that its input really was
/// a digit, and this one cannot be reached with a byte it has not classified.
const fn digit_value(byte: u8) -> Option<u128> {
    Some(match byte {
        b'0' => 0,
        b'1' => 1,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        b'5' => 5,
        b'6' => 6,
        b'7' => 7,
        b'8' => 8,
        b'9' => 9,
        _ => return None,
    })
}

/// Which overflow a magnitude that outgrew its accumulator is, given the sign already read.
const fn magnitude_overflow(negative: bool) -> ParseMoneyError {
    if negative {
        ParseMoneyError::NegativeMagnitudeOverflow
    } else {
        ParseMoneyError::PositiveMagnitudeOverflow
    }
}

/// Shift one decimal place and add `byte`'s digit.
///
/// Every step is checked: the text is untrusted, and an overflow here would otherwise be the
/// silent corruption this crate exists to prevent.
const fn push_digit(magnitude: u128, byte: u8, negative: bool) -> Result<u128, ParseMoneyError> {
    let Some(digit) = digit_value(byte) else {
        return Err(ParseMoneyError::InvalidSyntax);
    };
    let Some(shifted) = magnitude.checked_mul(10) else {
        return Err(magnitude_overflow(negative));
    };
    match shifted.checked_add(digit) {
        Some(value) => Ok(value),
        None => Err(magnitude_overflow(negative)),
    }
}
/// Parse a fixed-point decimal into canonical units. Liberal: any exact decimal.
///
/// Refuses excess precision rather than rounding.
///
/// `const`, so that a literal can be validated where it is written. Const evaluation admits no
/// iterators, no closures, no `?` and no `TryFrom`, so this reads bytes by index and matches
/// every `Result` explicitly. It remains the crate's **only** parser: a const twin could accept
/// a literal the runtime parser rejects, and nothing would notice until a golden moved.
pub(crate) const fn parse_fixed_point(text: &str) -> Result<i128, ParseMoneyError> {
    let bytes = text.as_bytes();
    let len = bytes.len();

    let negative = len > 0 && bytes[0] == b'-';
    let body_start = if negative { 1 } else { 0 };

    // One scan locates the point and counts the fraction. `split_once` took the *first* point
    // and let any second one fail the digit check below it; scanning reaches both verdicts
    // directly.
    let mut point = len; // `len` reads as "no point", so nothing has to unwrap an `Option`
    let mut supplied: u32 = 0;
    let mut index = body_start;
    while index < len {
        let byte = bytes[index];
        if byte == b'.' {
            // Belt and braces, and knowingly so: mutation-testing this guard away leaves every
            // verdict unchanged, because `point` would then track the LAST point and the
            // earlier one would fall inside the whole part, where `push_digit` rejects it. It
            // stays because refusing a second point *here* says why, at the byte that caused
            // it, rather than as a digit that turned out not to be one.
            if point != len {
                return Err(ParseMoneyError::InvalidSyntax);
            }
            point = index;
        } else if !byte.is_ascii_digit() {
            return Err(ParseMoneyError::InvalidSyntax);
        } else if point != len {
            // Counted as `u32` while scanning rather than narrowed from a `usize` afterwards:
            // `u32::try_from` is not const, and a fraction too long to count must keep its
            // original verdict of `InvalidSyntax` rather than becoming an overflow.
            supplied = match supplied.checked_add(1) {
                Some(count) => count,
                None => return Err(ParseMoneyError::InvalidSyntax),
            };
        }
        index = next(index);
    }

    // An empty whole part is refused, so `"-"`, `""` and `".5"` are all invalid syntax.
    if point == body_start {
        return Err(ParseMoneyError::InvalidSyntax);
    }
    if supplied > SCALE {
        return Err(ParseMoneyError::ExcessPrecision { digits: supplied });
    }

    // Right-pad the fraction to the canonical scale, then read whole and fraction as one
    // integer.
    let mut magnitude: u128 = 0;
    let mut whole = body_start;
    while whole < point {
        magnitude = match push_digit(magnitude, bytes[whole], negative) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        whole = next(whole);
    }

    let frac_start = if point == len { len } else { next(point) };
    let frac_len = len.checked_sub(frac_start).expect("the fraction starts at or before the end");
    let mut place = 0;
    while place < SCALE_USIZE {
        // The supplied digits first, then zero padding out to the canonical scale.
        let byte = if place < frac_len {
            bytes[frac_start.checked_add(place).expect("the fraction lies inside the input")]
        } else {
            b'0'
        };
        magnitude = match push_digit(magnitude, byte, negative) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        place = next(place);
    }

    // `i128::MIN` has no positive counterpart, which is why the accumulator is unsigned.
    if negative && magnitude == i128::MIN.unsigned_abs() {
        return Ok(i128::MIN);
    }
    if magnitude > I128_MAX_MAGNITUDE {
        return Err(magnitude_overflow(negative));
    }
    // The bound above is exactly what `i128::try_from` checks, so the two-complement bit
    // pattern is the value. Stated as a re-read of the bytes because the conversion is not
    // const and a cast is denied crate-wide; the guard is the proof, in code rather than prose.
    let units = i128::from_le_bytes(magnitude.to_le_bytes());
    if negative {
        Ok(units.checked_neg().expect("a positive i128 magnitude can always be negated"))
    } else {
        Ok(units)
    }
}

/// Parse the bare decimal carried by a structured rate representation.
///
/// Unlike [`parse_amount`](crate::text::parse_amount), this does not apply the money domain.
/// [`Rate`](crate::Rate)'s
/// constructor owns both the domain and strictly-positive checks, so every rate
/// ingress reaches the same validation edge.
#[cfg(feature = "serde")]
pub(crate) const fn parse_rate_amount(text: &str) -> Result<i128, ParseMoneyError> {
    parse_fixed_point(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AmountError;
    use crate::text::parse_amount;

    /// Parse each literal at compile time and again at run time, and require agreement.
    ///
    /// The `const` item is the point: it forces evaluation by the compiler rather than merely
    /// permitting it.
    macro_rules! agree {
    ($($name:ident => $literal:literal,)*) => {
        $(const $name: Result<i128, ParseMoneyError> = parse_amount($literal);)*

        #[test]
        fn the_const_and_runtime_parsers_are_one_parser() {
            $(
                assert_eq!(
                    $name,
                    parse_amount($literal),
                    "const and runtime evaluation disagreed on {:?}",
                    $literal,
                );
            )*
        }
    };
}

    agree! {
        // Accepted forms.
        ZERO => "0",
        PLAIN => "1500.00",
        NEGATIVE => "-1500.00",
        NEGATIVE_ZERO => "-0",
        ONE_UNIT => "0.000000000000000001",
        NEGATIVE_ONE_UNIT => "-0.000000000000000001",
        TRAILING_POINT => "5.",
        LEADING_ZEROS => "00000001.5",
        // The domain edges, both signs, at the full canonical scale.
        DOMAIN_TOP => "999999999999999999.999999999999999999",
        DOMAIN_BOTTOM => "-999999999999999999.999999999999999999",
        // Exactly the maximum scale, and one digit past it.
        MAX_SCALE => "1.123456789012345678",
        EXCESS_SCALE => "1.1234567890123456789",
        // One canonical unit outside the domain, each way.
        ABOVE_DOMAIN => "1000000000000000000",
        BELOW_DOMAIN => "-1000000000000000000",
        // Wide enough to exhaust the unsigned accumulator. If any step were unchecked, const
        // evaluation would refuse to build this file.
        ABSURDLY_WIDE => "9999999999999999999999999999999999999999999999999999999999999999",
        // Rejected forms. Each one is a syntax rule the runtime parser already enforced.
        EMPTY => "",
        LONE_SIGN => "-",
        LONE_POINT => ".",
        NO_WHOLE_PART => ".5",
        TWO_POINTS => "1.2.3",
        NOT_A_NUMBER => "abc",
        EXPONENT => "1e5",
        LEADING_SPACE => " 1",
        TRAILING_SPACE => "1 ",
        EXPLICIT_PLUS => "+1",
        INTERIOR_SIGN => "1.-2",
        UNDERSCORES => "1_000",
        TRAILING_SIGN => "1-",
    }

    // The corpus above proves the two evaluators agree. These pin what they agree *on*, so the
    // test cannot pass by having both paths be wrong in the same way.

    #[test]
    fn the_domain_edge_parses_to_the_domain_edge() {
        assert_eq!(DOMAIN_TOP, Ok(DOMAIN_MAX));
        assert_eq!(DOMAIN_BOTTOM, Ok(-DOMAIN_MAX));
        assert_eq!(
            ABOVE_DOMAIN,
            Err(ParseMoneyError::Amount(AmountError::out_of_domain(
                1_000_000_000_000_000_000_000_000_000_000_000_000
            )))
        );
    }

    #[test]
    fn the_scale_boundary_is_the_scale_and_not_one_either_side() {
        assert_eq!(MAX_SCALE, Ok(1_123_456_789_012_345_678));
        assert_eq!(EXCESS_SCALE, Err(ParseMoneyError::ExcessPrecision { digits: 19 }));
    }

    #[test]
    fn a_magnitude_too_wide_for_the_accumulator_is_an_overflow_not_a_wrap() {
        // Reported as an overflow rather than silently reduced modulo anything, and reported with
        // the sign that was read, because the two overflow variants are distinguishable.
        assert_eq!(ABSURDLY_WIDE, Err(ParseMoneyError::PositiveMagnitudeOverflow));
    }

    #[test]
    fn the_rejected_forms_are_rejected_for_the_stated_reason() {
        for rejected in [EMPTY, LONE_SIGN, LONE_POINT, NO_WHOLE_PART, TWO_POINTS, NOT_A_NUMBER] {
            assert_eq!(rejected, Err(ParseMoneyError::InvalidSyntax));
        }
        for rejected in [EXPONENT, LEADING_SPACE, TRAILING_SPACE, EXPLICIT_PLUS, INTERIOR_SIGN] {
            assert_eq!(rejected, Err(ParseMoneyError::InvalidSyntax));
        }
        for rejected in [UNDERSCORES, TRAILING_SIGN] {
            assert_eq!(rejected, Err(ParseMoneyError::InvalidSyntax));
        }
        // A trailing point is NOT rejected. The runtime parser accepted it before this work, and a
        // const rewrite that quietly tightened the grammar would be a behaviour change wearing the
        // clothes of a refactor.
        assert_eq!(TRAILING_POINT, Ok(5_000_000_000_000_000_000));
    }

    // `money!` itself. Every one of these is a `const`, so the macro is doing its work at compile
    // time rather than being tested only as an expression.
}
