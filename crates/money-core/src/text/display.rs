//! `Display` and `FromStr` for `Money` and `Rate`, both over the shared codec.

use super::parse::{parse_amount, parse_fixed_point, split_tagged};
use super::render::{render, render_fixed_point};
use crate::Money;
use crate::Rate;
use crate::StaticCurrency;
use crate::errors::{CurrencyMismatch, ParseMoneyError, RateError};
use crate::iso::Iso4217;
use core::fmt;
use core::str::FromStr;

impl<C: StaticCurrency> fmt::Display for Money<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Delegates to the same [`render`] a runtime-currency caller gets. Two implementations
        // agreeing today is not the same property as one implementation.
        //
        // The `expect` is unreachable: `Money<C>` cannot hold out-of-domain units — every
        // constructor goes through `try_from_units`, and the raw `i128` is not publicly reachable.
        // That is precisely why `render` takes the check and `Display` does not need one.
        let rendered = render(self.units(), C::CODE).expect("Money<C> is in-domain by construction");
        pad_without_truncating(f, &rendered)
    }
}

/// Honour width and alignment without letting formatter precision truncate the canonical text.
///
/// Currency policy owns fractional digits. With no formatting options, `pad` is byte-identical
/// to `write_str`.
fn pad_without_truncating(f: &mut fmt::Formatter<'_>, rendered: &str) -> fmt::Result {
    if f.precision().is_some() { f.write_str(rendered) } else { f.pad(rendered) }
}
/// A rate is a RATIO, not an amount, so no currency's minor unit governs it: it trims all the
/// way down to the last significant digit. `USD/IDR/16000`, never `USD/IDR/16000.00`.
///
/// Shape is ISO 15022 field 92B's: `:4!c//3!a/3!a/15d`, e.g. `:92B::EXCH//GBP/USD/1,619`
/// meaning 1,00 GBP = 1,619 USD — first code the BASE, second the QUOTE. One deviation,
/// deliberate: the decimal separator is a point, not SWIFT's comma, for exact decimal transport
/// into JavaScript, where `parseFloat("1,619")`
/// is `1`.
impl<Base: StaticCurrency, Quote: StaticCurrency> fmt::Display for Rate<Base, Quote> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rendered = format!(
            "{}/{}/{}",
            Base::CODE.alpha3(),
            Quote::CODE.alpha3(),
            render_fixed_point(self.units(), 0)
        );
        pad_without_truncating(f, &rendered)
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> FromStr for Rate<Base, Quote> {
    type Err = RateError;

    /// Parse `"<BASE>/<QUOTE>/<rate>"`, checking **both** ends of the pair.
    ///
    /// # Errors
    /// [`ParseMoneyError::WrongCurrency`] if either end disagrees; the base is reported first,
    /// because accepting a reversed pair would invert the price — the one error a quote feed
    /// can make that still looks like a number.
    /// Other parse failures are wrapped by [`RateError::Parse`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('/');
        // The trailing `None` is load-bearing: it rejects "USD/IDR/1/2". Without it a fourth
        // field would be silently ignored, which on a wire is how a value gets misread.
        let (Some(base), Some(quote), Some(amount), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ParseMoneyError::InvalidSyntax.into());
        };

        let base = Iso4217::from_alpha3(base).ok_or(ParseMoneyError::InvalidSyntax)?;
        if base != Base::CODE {
            return Err(ParseMoneyError::WrongCurrency(CurrencyMismatch::new(Base::CODE, base)).into());
        }
        let quote = Iso4217::from_alpha3(quote).ok_or(ParseMoneyError::InvalidSyntax)?;
        if quote != Quote::CODE {
            return Err(ParseMoneyError::WrongCurrency(CurrencyMismatch::new(Quote::CODE, quote)).into());
        }

        let units = parse_fixed_point(amount)?;
        // Preserve the constructor's distinction between magnitude and sign failures.
        Self::try_from_units(units)
    }
}

impl<C: StaticCurrency> FromStr for Money<C> {
    type Err = ParseMoneyError;

    /// Parse `"<ISO> <amount>"`.
    ///
    /// Liberal in what it accepts — any exact decimal, canonical or not — and strict in what
    /// it refuses. In particular it **never rounds**: more fractional digits than [`SCALE`](crate::advanced::domain::SCALE)
    /// is [`ParseMoneyError::ExcessPrecision`], not a quietly truncated value.
    ///
    /// # Errors
    /// [`ParseMoneyError::WrongCurrency`] if the code does not match `C`. This redundancy is the
    /// point: it catches an IDR amount arriving in a USD field.
    /// [`ParseMoneyError::InvalidSyntax`] for malformed input;
    /// [`ParseMoneyError::ExcessPrecision`] for more than [`SCALE`](crate::advanced::domain::SCALE) fractional digits;
    /// a sign-specific magnitude overflow when the canonical-unit value cannot fit `i128`;
    /// and [`ParseMoneyError::Amount`] outside the fixed domain.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split first, compare the currency, and only then read the digits — so
        // `"IDR not-a-number"` into a `Money<USD>` reports the currency, which is the useful
        // error at a boundary. Calling `parse` here instead would report the digits.
        let (code, amount) = split_tagged(s)?;
        if code != C::CODE {
            return Err(ParseMoneyError::WrongCurrency(CurrencyMismatch::new(C::CODE, code)));
        }

        let units = parse_amount(amount)?;
        Self::try_from_units(units).map_err(ParseMoneyError::from)
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{DOMAIN_MAX, POW10_SCALE};
    use crate::errors::CurrencyMismatch;
    use crate::errors::ParseMoneyError;
    use crate::iso::{IDR, JPY, KWD, USD};
    use crate::{Iso4217, Money, Rate};
    use proptest::prelude::*;
    use std::str::FromStr;

    fn usd(units: i128) -> Money<USD> {
        Money::<USD>::try_from_units(units).unwrap()
    }

    /// Parse is LIBERAL: it accepts any exact decimal, not only the canonical spelling.
    ///
    /// The property is `parse(render(v)) == v`, not a two-way textual identity.
    #[test]
    fn parse_accepts_non_canonical_spellings_of_the_same_value() {
        let canonical = usd(10_500_000_000_000_000_000);
        for spelling in ["USD 10.50", "USD 10.5", "USD 10.500000", "USD 10.500000000000000000"] {
            assert_eq!(Money::<USD>::from_str(spelling).unwrap(), canonical, "{spelling}");
        }
        // Whole amounts with no point at all.
        assert_eq!(Money::<USD>::from_str("USD 10").unwrap(), usd(10 * POW10_SCALE));
    }
    /// The currency in the string is a CROSS-CHECK, not decoration. It is what catches an IDR
    /// value landing in a USD field at an API boundary, which is exactly where types cannot help.
    #[test]
    fn parsing_the_wrong_currency_is_an_error() {
        assert_eq!(
            Money::<USD>::from_str("IDR 10.50"),
            Err(ParseMoneyError::WrongCurrency(CurrencyMismatch::new(
                crate::Iso4217::USD,
                crate::Iso4217::IDR
            )))
        );
    }
    #[test]
    fn malformed_input_is_refused_rather_than_guessed() {
        for bad in [
            "",
            "USD",
            "10.50",                      // no currency
            "ZZZ 10.50",                  // not a currency
            "USD 10.50.50",               // two points
            "USD ten",                    // not a number
            "USD 10,50",                  // decimal COMMA is not this crate's separator
            "USD 0.0000000000000000001",  // 19dp: below one canonical unit, so NOT representable
            "USD 1000000000000000000.00", // 10^18: one major unit past the domain
        ] {
            assert!(Money::<USD>::from_str(bad).is_err(), "must refuse {bad:?}");
        }
    }
    /// Excess precision is refused, never silently rounded.
    ///
    /// A money parser must not turn a distinct over-precise input into an accepted value.
    #[test]
    fn excess_precision_is_refused_not_rounded() {
        assert!(Money::<USD>::from_str("USD 0.0000000000000000005").is_err());
        assert!(Money::<IDR>::from_str("IDR 1.1234567890123456789").is_err());
    }
    #[test]
    fn a_rate_renders_as_base_slash_quote_slash_rate() {
        let r = Rate::<USD, IDR>::try_from_units(16_000 * POW10_SCALE).unwrap();
        assert_eq!(r.to_string(), "USD/IDR/16000");
    }
    /// A rate is a RATIO, not an amount, so no currency's minor unit governs it: it trims all the
    /// way to the last significant digit. `USD/IDR/16000`, never `USD/IDR/16000.00`.
    #[test]
    fn a_rate_has_no_settlement_exponent_so_it_trims_fully() {
        let whole = Rate::<USD, IDR>::try_from_units(16_000 * POW10_SCALE).unwrap();
        assert_eq!(whole.to_string(), "USD/IDR/16000");

        let fractional = Rate::<USD, IDR>::try_from_units(15_432_100_000_000_000_000_000).unwrap();
        assert_eq!(fractional.to_string(), "USD/IDR/15432.1");

        // Even for a pair whose currencies both have exponents.
        let kwd = Rate::<KWD, JPY>::try_from_units(POW10_SCALE / 2).unwrap();
        assert_eq!(kwd.to_string(), "KWD/JPY/0.5");
    }
    #[test]
    fn a_rate_round_trips_and_checks_both_ends_of_the_pair() {
        let r = Rate::<USD, IDR>::try_from_units(16_000 * POW10_SCALE).unwrap();
        assert_eq!(Rate::<USD, IDR>::from_str(&r.to_string()).unwrap(), r);

        assert!(Rate::<JPY, IDR>::from_str("USD/IDR/16000").is_err(), "the BASE end must be checked");
        assert!(
            Rate::<USD, JPY>::from_str("USD/IDR/16000").is_err(),
            "the QUOTE end must be checked too — a rate is directed"
        );
    }
    #[test]
    fn a_rate_refuses_malformed_text() {
        for bad in [
            "USD/IDR",         // no rate
            "USD 16000",       // money shape, not rate shape
            "USD/IDR/16000/2", // too many fields
            "USDIDR/16000",    // pair not separated
            "USD/IDR/16,000",  // SWIFT's decimal comma is NOT this crate's separator
            "USD/ZZZ/16000",   // not a currency
        ] {
            assert!(Rate::<USD, IDR>::from_str(bad).is_err(), "must refuse {bad:?}");
        }
    }

    proptest! {
        /// `parse(render(v)) == v` for every value in the domain. The converse does not hold:
        /// `render(parse(s)) == s` fails for "USD 10.5", which parses fine but renders as
        /// "USD 10.50".
        #[test]
        fn prop_parse_of_render_is_the_identity(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let m = usd(units);
            prop_assert_eq!(Money::<USD>::from_str(&m.to_string()).unwrap(), m);
        }

        /// The same, for a currency whose settlement exponent is 0 — the case where trimming
        /// removes the decimal point entirely and the parser must cope with a bare integer.
        #[test]
        fn prop_parse_of_render_is_the_identity_at_exponent_zero(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let m = Money::<JPY>::try_from_units(units).unwrap();
            prop_assert_eq!(Money::<JPY>::from_str(&m.to_string()).unwrap(), m);
        }

        /// Re-rendering a parsed canonical string changes nothing.
        #[test]
        fn prop_render_is_idempotent_through_a_parse(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let once = usd(units).to_string();
            let twice = Money::<USD>::from_str(&once).unwrap().to_string();
            prop_assert_eq!(once, twice);
        }
    }

    proptest! {
        /// Rates are strictly positive.
        #[test]
        fn prop_rate_parse_of_render_is_the_identity(units in 1..=DOMAIN_MAX) {
            let r = Rate::<USD, IDR>::try_from_units(units).unwrap();
            prop_assert_eq!(Rate::<USD, IDR>::from_str(&r.to_string()).unwrap(), r);
        }
    }

    /// Short and zero-extended fractional forms must produce the same units.
    #[test]
    fn short_and_padded_fractions_agree() {
        let a = Money::<USD>::from_str("USD 1.5").unwrap();
        let b = Money::<USD>::from_str("USD 1.500000000000000000").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.units(), 1_500_000_000_000_000_000);
    }

    #[test]
    fn the_currency_cross_check_fires_before_the_amount_is_read() {
        // "not a number" would also fail, but the CURRENCY error is the useful one to get
        // back: it names the actual mistake at an API boundary.
        assert_eq!(
            Money::<USD>::from_str("IDR not-a-number"),
            Err(ParseMoneyError::WrongCurrency(CurrencyMismatch::new(Iso4217::USD, Iso4217::IDR)))
        );
    }

    #[test]
    fn display_and_parse_agree_on_the_domain_edges() {
        for units in [crate::domain::DOMAIN_MAX, -crate::domain::DOMAIN_MAX, 0, 1, -1] {
            let m = Money::<IDR>::try_from_units(units).unwrap();
            assert_eq!(Money::<IDR>::from_str(&m.to_string()).unwrap(), m, "{units}");
        }
    }
}
