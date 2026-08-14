//! One parser, proved to answer the same in both evaluation modes.
//!
//! `text::parse_amount` is `const`, so a literal can be checked where it is written. That is
//! only worth anything while the const path and the runtime path stay the *same* path. The
//! risk a single implementation still carries is that the two evaluators disagree: const
//! evaluation refuses an arithmetic overflow as a build error, while a release build wraps and
//! carries on. A parser that overflowed on a long digit string would therefore fail to compile
//! here and silently produce a wrong amount in production.
//!
//! Every entry in `AGREE` below is parsed twice — once as a `const` item, which the compiler
//! must evaluate before this file builds, and once by an ordinary call — and the two results
//! are compared. A const-evaluation failure is a build failure, so the corpus not compiling is
//! itself the negative result.

use kamu_money_core::Money;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::errors::{AmountError, ParseMoneyError};
use kamu_money_core::iso::USD;
use kamu_money_core::money;
use kamu_money_core::text::parse_amount;

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

const RENT: Money<USD> = money!(USD, "1500.00");
const REFUND: Money<USD> = money!(USD, "-1500.00");
const SMALLEST: Money<USD> = money!(USD, "0.000000000000000001");
const EDGE: Money<USD> = money!(USD, "999999999999999999.999999999999999999");

#[test]
fn the_macro_reads_a_literal_as_the_amount_a_reviewer_reads() {
    assert_eq!(RENT.units(), 1_500_000_000_000_000_000_000);
    assert_eq!(REFUND.units(), -1_500_000_000_000_000_000_000);
    assert_eq!(SMALLEST.units(), 1);
    assert_eq!(EDGE.units(), DOMAIN_MAX);
}

#[test]
fn the_macro_and_the_runtime_parser_cannot_disagree() {
    // The whole reason `parse_fixed_point` gained `const` instead of gaining a const twin.
    // `FromStr` reads the *tagged* form, because it checks the currency code against `C` before
    // it reads any digits; `money!` takes the currency as a type, so its literal is bare. Both
    // reach the same parser underneath, which is what this compares.
    for (tagged, expected) in [
        ("USD 1500.00", RENT),
        ("USD -1500.00", REFUND),
        ("USD 0.000000000000000001", SMALLEST),
        ("USD 999999999999999999.999999999999999999", EDGE),
    ] {
        let parsed: Money<USD> = tagged.parse().expect("the macro accepted this amount");
        assert_eq!(parsed, expected, "FromStr and money! disagreed on {tagged:?}");
    }
}

#[test]
fn the_macro_is_usable_as_an_ordinary_expression() {
    assert_eq!((money!(USD, "10.50") + money!(USD, "0.50")).units(), 11_000_000_000_000_000_000);
}
