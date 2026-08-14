//! The tagged form: `"<ISO> <amount>"`, split before the digits are read.

use super::fixed_point::parse_fixed_point;
use crate::domain::in_domain;
use crate::errors::{AmountError, ParseMoneyError};
use crate::iso::Iso4217;

/// Split `"<ISO> <amount>"` into its currency and its still-unparsed amount.
///
/// Separate from [`parse`](crate::text::parse()) so that [`Money`](crate::Money)'s `FromStr` can compare the code against `C`
/// *before* reading the digits. Getting `WrongCurrency` back from `"IDR not-a-number"` is more
/// use at an API boundary than `InvalidSyntax`, and a single combined parser could not offer
/// that ordering to one caller and the plain answer to the other.
pub(crate) fn split_tagged(text: &str) -> Result<(Iso4217, &str), ParseMoneyError> {
    let (code_text, amount) = text.split_once(' ').ok_or(ParseMoneyError::InvalidSyntax)?;
    let code = Iso4217::from_alpha3(code_text).ok_or(ParseMoneyError::InvalidSyntax)?;
    Ok((code, amount))
}
/// Parse a bare amount — no currency prefix — into domain-checked canonical units.
///
/// # Errors
/// [`ParseMoneyError::InvalidSyntax`] for malformed input,
/// [`ParseMoneyError::ExcessPrecision`] above [`SCALE`](crate::advanced::domain::SCALE) fractional digits,
/// [`ParseMoneyError::PositiveMagnitudeOverflow`] or
/// [`ParseMoneyError::NegativeMagnitudeOverflow`] when the signed canonical-unit
/// value cannot fit `i128`,
/// and [`ParseMoneyError::Amount`] outside the money domain.
/// `const`, so a literal can be checked where it is written — which is what
/// [`money!`](crate::money!) is built on. It remains the crate's only parser: a const twin could
/// accept a literal this one rejects, and nothing would notice until a golden moved.
pub const fn parse_amount(text: &str) -> Result<i128, ParseMoneyError> {
    let units = match parse_fixed_point(text) {
        Ok(units) => units,
        Err(error) => return Err(error),
    };
    // parse_fixed_point stops at what an i128 can hold; the DOMAIN is narrower. The generic
    // path got this check from `Money::try_from_units`, so a caller taking raw units needs it
    // applied here or the two paths would not agree on what is in range. Same `in_domain`
    // that `try_from_units` calls, rather than a restatement of the bound.
    if !in_domain(units) {
        // `Into` is not const, so the variant `#[from]` generates is named directly.
        return Err(ParseMoneyError::Amount(AmountError::out_of_domain(units)));
    }
    Ok(units)
}

/// Parse `"<ISO> <amount>"` without knowing the currency at compile time.
///
/// For adapters that must accept **whatever currency arrives** — a database type, a wire
/// boundary — where [`Money`](crate::Money)'s `FromStr` cannot be used because there is no `C` to name.
/// Returns the currency alongside the units rather than resolving it, because deciding what a
/// mismatch means is the caller's job: `Money<C>`'s `FromStr` treats it as an error, while a
/// storage type stores it.
///
/// # Errors
/// As [`parse_amount`], plus [`ParseMoneyError::InvalidSyntax`] if the string is not
/// `<code> <amount>` or the code is not a known ISO 4217 alpha-3.
pub fn parse(text: &str) -> Result<(Iso4217, i128), ParseMoneyError> {
    let (code, amount) = split_tagged(text)?;
    Ok((code, parse_amount(amount)?))
}

#[cfg(test)]
mod tests {
    use crate::Rounding;
    use crate::arithmetic::{allocate_units, div_int_units};
    use crate::domain::DOMAIN_MAX;
    use crate::errors::{AmountError, ParseMoneyError};
    use crate::iso::USD;
    use crate::{Iso4217, Money, text};
    use core::num::NonZeroU32;
    use proptest::prelude::*;
    use std::str::FromStr;

    fn usd(units: i128) -> Money<USD> {
        Money::<USD>::try_from_units(units).unwrap()
    }

    /// `parse` returns the currency instead of judging it. That is the whole difference from
    /// `FromStr`, which has a `C` to compare against and treats a mismatch as an error.
    #[test]
    fn the_runtime_codec_accepts_any_currency_and_reports_which() {
        assert_eq!(text::parse("IDR 10.50").unwrap(), (Iso4217::IDR, 10_500_000_000_000_000_000));
        assert_eq!(text::parse("USD 10.50").unwrap(), (Iso4217::USD, 10_500_000_000_000_000_000));

        // The same string `Money::<USD>::from_str` rejects with WrongCurrency.
        assert!(Money::<USD>::from_str("IDR 10.50").is_err());
    }
    /// The two parsers agree on the units for anything they both accept.
    #[test]
    fn the_runtime_codec_parses_the_same_units_as_from_str() {
        for text in ["USD 0", "USD 10.5", "USD 10.50", "USD -0.000000000000000001"] {
            let (code, units) = text::parse(text).unwrap();
            assert_eq!(code, Iso4217::USD);
            assert_eq!(units, Money::<USD>::from_str(text).unwrap().units());
        }
    }
    /// An unknown alpha-3 is refused, not stored as some placeholder. `kamu-money-pg` relies on this:
    /// it is the only thing standing between an unrecognised code and a stored amount whose
    /// currency is a guess.
    #[test]
    fn the_runtime_codec_refuses_a_currency_it_does_not_know() {
        for bad in ["ZWL 1.00", "ZZZ 1.00", "US 1.00", "USDD 1.00", "1.00", "USD"] {
            assert!(text::parse(bad).is_err(), "must refuse {bad:?}");
        }
    }
    /// `parse_amount` enforces the domain, which the generic path got from `Money::try_from_units`.
    /// Without it a caller taking raw units would accept values `Money` refuses.
    #[test]
    fn parse_amount_enforces_the_domain_on_raw_units() {
        assert_eq!(text::parse_amount("0.000000000000000001").unwrap(), 1);
        assert_eq!(text::parse_amount("999999999999999999.999999999999999999").unwrap(), DOMAIN_MAX);
        assert_eq!(
            text::parse_amount("1000000000000000000"),
            Err(ParseMoneyError::Amount(AmountError::out_of_domain(DOMAIN_MAX + 1)))
        );
        assert_eq!(
            text::parse_amount("-1000000000000000000"),
            Err(ParseMoneyError::Amount(AmountError::out_of_domain(-DOMAIN_MAX - 1)))
        );
        assert!(matches!(
            text::parse_amount("0.0000000000000000004"),
            Err(ParseMoneyError::ExcessPrecision { digits: 19 })
        ));
    }
    #[test]
    fn parser_preserves_sign_when_magnitude_exceeds_i128() {
        let positive_overflow = "170141183460469231731.687303715884105728";
        assert_eq!(text::parse_amount(positive_overflow), Err(ParseMoneyError::PositiveMagnitudeOverflow));

        let exact_minimum = "-170141183460469231731.687303715884105728";
        assert_eq!(
            text::parse_amount(exact_minimum),
            Err(ParseMoneyError::Amount(AmountError::out_of_domain(i128::MIN)))
        );

        let negative_overflow = "-170141183460469231731.687303715884105729";
        assert_eq!(text::parse_amount(negative_overflow), Err(ParseMoneyError::NegativeMagnitudeOverflow));
    }
    /// The raw-unit entry points documented their domain precondition and did not enforce it.
    /// `i128::MAX` went in and out-of-domain values came back — parts no `Money` constructor
    /// would admit, returned as though they were money.
    #[test]
    fn the_raw_unit_entry_points_refuse_values_no_money_could_hold() {
        let three = NonZeroU32::new(3).unwrap();
        for out_of_domain in [i128::MAX, i128::MIN, DOMAIN_MAX + 1, -DOMAIN_MAX - 1] {
            assert!(text::render(out_of_domain, Iso4217::USD).is_err(), "render accepted {out_of_domain}");
            assert!(
                allocate_units(out_of_domain, &[1, 1]).is_err(),
                "allocate_units accepted {out_of_domain}"
            );
            assert!(
                div_int_units(out_of_domain, three, Rounding::TowardZero).is_err(),
                "div_int_units accepted {out_of_domain}"
            );
        }

        // ...and the domain edges themselves still work. A check that rejected everything would
        // pass the assertions above while breaking every real caller.
        for edge in [DOMAIN_MAX, -DOMAIN_MAX, 0, 1, -1] {
            assert!(text::render(edge, Iso4217::USD).is_ok(), "edge {edge}");
            assert!(allocate_units(edge, &[1, 1]).is_ok(), "edge {edge}");
            assert!(div_int_units(edge, three, Rounding::TowardZero).is_ok(), "edge {edge}");
        }
    }

    proptest! {
        /// The retraction holds for the runtime codec too, for every currency in the table —
        /// including the ones whose settlement exponent is 0 or 3, where the trim rule does the
        /// most work.
        #[test]
        fn prop_runtime_codec_parse_of_render_is_the_identity(
            units in -DOMAIN_MAX..=DOMAIN_MAX,
            which in 0usize..4,
        ) {
            let currency = [Iso4217::USD, Iso4217::JPY, Iso4217::KWD, Iso4217::XAU][which];
            let rendered = text::render(units, currency).unwrap();
            prop_assert_eq!(text::parse(&rendered).unwrap(), (currency, units));
        }

        /// The generic and runtime paths render identically for every value, not just the
        /// hand-picked ones above.
        #[test]
        fn prop_runtime_and_generic_render_agree(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            prop_assert_eq!(text::render(units, Iso4217::USD).unwrap(), usd(units).to_string());
        }

        /// Whatever the renderer accepts, the parser must accept back, across the full `i128`
        /// input range.
        #[test]
        fn prop_render_never_emits_text_its_own_parser_refuses(
            units in i128::MIN..=i128::MAX,
            which in 0usize..4,
        ) {
            let currency = [Iso4217::USD, Iso4217::JPY, Iso4217::KWD, Iso4217::XAU][which];
            match text::render(units, currency) {
                Ok(rendered) => prop_assert_eq!(
                    text::parse(&rendered).unwrap(),
                    (currency, units),
                    "rendered {} but the parser would not take it back", rendered
                ),
                // Refusing is the correct answer outside the domain; it must not be silent.
                Err(_) => prop_assert!(!crate::domain::in_domain(units)),
            }
        }
    }
}
