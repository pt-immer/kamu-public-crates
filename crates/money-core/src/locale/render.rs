//! Applying a policy to an amount. Reads the policy through its accessors, so rendering
//! cannot depend on a field the public surface does not expose.

use super::group::group;
use super::{LocalePolicy, SymbolPosition};
use crate::Money;
use crate::StaticCurrency;
use crate::errors::{AmountError, CurrencyMismatch, LocaleError};
use crate::iso::Iso4217;
use crate::text::fixed_point_parts;

impl LocalePolicy<'_> {
    /// Render a `Money<C>` under this policy.
    ///
    /// # Errors
    /// [`LocaleError::WrongCurrency`] if `C` is not this policy's currency.
    pub fn render<C: StaticCurrency>(&self, money: Money<C>) -> Result<String, LocaleError> {
        self.render_units(money.units(), C::CODE)
    }
    /// Render raw units for a currency known only at **run time**.
    ///
    /// The non-generic twin, for the same callers [`crate::text::render`] exists for: a
    /// database row or a wire message whose currency is a value rather than a type. Shares
    /// the generic path's implementation rather than resembling it.
    ///
    /// # Errors
    /// [`LocaleError::WrongCurrency`] if `currency` is not this policy's currency, and
    /// [`LocaleError::Amount`] if `units` is outside the domain.
    ///
    /// The domain arm matters more here than anywhere else in the crate: this is the one
    /// function whose output a **human reads and acts on**. Returning `Ok` for an amount the
    /// type cannot hold would put a number in front of a person that no other layer would
    /// accept — the failure this crate exists to prevent, arriving through its last surface.
    pub fn render_units(&self, units: i128, currency: Iso4217) -> Result<String, LocaleError> {
        if currency != self.currency() {
            return Err(LocaleError::WrongCurrency(CurrencyMismatch::new(self.currency(), currency)));
        }
        if !crate::domain::in_domain(units) {
            return Err(AmountError::out_of_domain(units).into());
        }

        let (negative, whole, fraction) =
            fixed_point_parts(units, usize::from(self.min_fraction_digits().get()));

        let grouped = group(&whole, self.grouping(), self.group_separator());
        let body = if fraction.is_empty() {
            grouped
        } else {
            format!("{grouped}{}{fraction}", self.decimal_separator())
        };

        let decorated = match self.symbol_position() {
            SymbolPosition::Prefix => format!("{}{body}", self.symbol()),
            SymbolPosition::Suffix => format!("{body}{}", self.symbol()),
        };

        // The sign is outermost, ahead of a prefixed symbol: `-$1,234.50`. One rule for both
        // placements, so a reader never has to hunt for it.
        Ok(if negative { format!("-{decorated}") } else { decorated })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{DOMAIN_MAX, POW10_SCALE};
    use crate::errors::CurrencyMismatch;
    use crate::errors::LocaleError;
    use crate::iso::{EUR, IDR, JPY, USD, XAU};
    use crate::locale::{
        DE_EUR, EN_USD, FractionDigits, ID_IDR, JA_JPY, LocalePolicy, SymbolPosition, idr_16000_50,
    };
    use crate::{Iso4217, Money, text};
    use proptest::prelude::*;

    /// **The headline property.** Applying a policy does not move the canonical form.
    ///
    /// Deliberately asserts the *same literals* the text, wire, and database suites assert, so
    /// that if `LocalePolicy` ever grew a path through `Display` this test would go red beside
    /// them rather than instead of them.
    #[test]
    fn the_canonical_form_is_untouched_by_any_policy() {
        let m = idr_16000_50();
        assert_eq!(m.to_string(), "IDR 16000.50", "settlement dp, unchanged");
        assert_eq!(ID_IDR.render(m).unwrap(), "Rp 16.000,5");
        assert_eq!(m.to_string(), "IDR 16000.50", "and still unchanged after rendering");

        let usd = Money::<USD>::try_from_units(1_234_500_000_000_000_000_000).unwrap();
        assert_eq!(usd.to_string(), "USD 1234.50");
        assert_eq!(EN_USD.render(usd).unwrap(), "$1,234.50");
    }
    /// IDR renders with a zero-digit minimum but settles at two decimal places.
    #[test]
    fn rupiah_displays_at_zero_dp_but_settles_at_two() {
        assert_eq!(Iso4217::IDR.exponent(), Some(2), "ISO settlement exponent is 2");
        let whole = Money::<IDR>::try_from_major(16_000).unwrap();
        assert_eq!(whole.to_string(), "IDR 16000.00", "settles at 2");
        assert_eq!(ID_IDR.render(whole).unwrap(), "Rp 16.000", "displays at 0");
    }
    /// A display minimum below the value's significant digits must
    /// pad nothing and drop nothing. `16000.50` at a 0dp policy is `16.000,5`, never `16.000`.
    ///
    /// Truncating the rendered fraction must make this test fail.
    #[test]
    fn a_policy_pads_but_never_rounds() {
        assert_eq!(ID_IDR.min_fraction_digits(), FractionDigits::ZERO);

        // One significant digit past the display minimum.
        assert_eq!(ID_IDR.render(idr_16000_50()).unwrap(), "Rp 16.000,5");

        // Eighteen of them: the smallest representable value must survive intact.
        let dust = Money::<IDR>::try_from_units(1).unwrap();
        assert_eq!(ID_IDR.render(dust).unwrap(), "Rp 0,000000000000000001");

        // And a value whose digits stop short of the minimum is PADDED up to it.
        let flat = Money::<USD>::try_from_major(7).unwrap();
        assert_eq!(EN_USD.render(flat).unwrap(), "$7.00");
    }
    /// Symbol placement and separator choice are the locale's, not the currency's.
    #[test]
    fn the_symbol_may_follow_the_amount() {
        let m = Money::<EUR>::try_from_units(1_234_500_000_000_000_000_000).unwrap();
        assert_eq!(m.to_string(), "EUR 1234.50", "canonical is unaffected");
        assert_eq!(DE_EUR.render(m).unwrap(), "1.234,50 €");
        assert_eq!(DE_EUR.symbol_position(), SymbolPosition::Suffix);
    }
    /// A currency with a settlement exponent of zero needs no special case: the default
    /// minimum is the ISO exponent, so JPY arrives at 0.
    #[test]
    fn a_zero_exponent_currency_needs_no_special_case() {
        let m = Money::<JPY>::try_from_major(1_234).unwrap();
        assert_eq!(m.to_string(), "JPY 1234");
        assert_eq!(JA_JPY.render(m).unwrap(), "￥1,234");

        // ...and a fractional yen still shows every digit it has.
        let odd = Money::<JPY>::try_from_units(1_234 * POW10_SCALE + 250_000_000_000_000_000).unwrap();
        assert_eq!(JA_JPY.render(odd).unwrap(), "￥1,234.25");
    }
    /// `exponent()` is `Option<u8>`; `None` means "no minor unit", which is zero places.
    #[test]
    fn a_currency_with_no_minor_unit_defaults_to_no_fraction() {
        let policy = LocalePolicy::new(Iso4217::XAU, "XAU");
        assert_eq!(Iso4217::XAU.exponent(), None, "gold has no cents");
        assert_eq!(policy.min_fraction_digits(), FractionDigits::ZERO);
        let gold = Money::<XAU>::try_from_units(10_500_000_000_000_000_000).unwrap();
        assert_eq!(gold.to_string(), "XAU 10.5");
        // Prefix is the default position, so the alpha-3 used as a symbol leads.
        assert_eq!(policy.render(gold).unwrap(), "XAU10.5");
    }
    /// The cross-check that `FromStr` already performs on the way in, performed on the way out.
    /// Formatting a `Money<USD>` through the rupiah policy would silently emit `Rp`, which is a
    /// mislabelled number reaching a human — the failure this whole crate is organised around.
    #[test]
    fn a_policy_refuses_a_currency_that_is_not_its_own() {
        let usd = Money::<USD>::try_from_major(10).unwrap();
        assert_eq!(
            ID_IDR.render(usd),
            Err(LocaleError::WrongCurrency(CurrencyMismatch::new(Iso4217::IDR, Iso4217::USD)))
        );
        // Same check on the runtime path, where the currency is a value rather than a type.
        assert!(ID_IDR.render_units(1, Iso4217::USD).is_err());
        assert!(ID_IDR.render_units(1, Iso4217::IDR).is_ok());
    }
    /// The sign is outermost, ahead of a prefixed symbol and ahead of the digits either way.
    #[test]
    fn the_sign_sits_outside_the_symbol() {
        let owed = Money::<USD>::try_from_units(-1_234_500_000_000_000_000_000).unwrap();
        assert_eq!(EN_USD.render(owed).unwrap(), "-$1,234.50");

        let owed_eur = Money::<EUR>::try_from_units(-1_234_500_000_000_000_000_000).unwrap();
        assert_eq!(DE_EUR.render(owed_eur).unwrap(), "-1.234,50 €");
    }
    /// One digit engine, not two. A policy configured to match the canonical form's choices must
    /// reproduce its digits exactly, for **every** currency in the table — which is only possible
    /// if both are reading the same implementation rather than agreeing by coincidence.
    ///
    /// Compares the digits, not the whole string: the two forms place the sign differently on
    /// purpose (`USD -10.50` against `-USD10.50`), and that is a decoration difference, which is
    /// precisely what a policy is allowed to have.
    #[test]
    fn a_canonical_shaped_policy_reproduces_the_canonical_digits() {
        for &code in Iso4217::EVERY {
            let policy = LocalePolicy::new(code, code.alpha3())
                .with_symbol_position(SymbolPosition::Prefix)
                .try_with_grouping(&[])
                .unwrap()
                .try_with_separators("", ".")
                .unwrap();
            for units in [0, 1, -1, DOMAIN_MAX, -DOMAIN_MAX, 10_500_000_000_000_000_000] {
                let via_policy = policy.render_units(units, code).unwrap().replace(code.alpha3(), "");
                let canonical = text::render(units, code)
                    .unwrap()
                    .strip_prefix(code.alpha3())
                    .expect("canonical form leads with the alpha-3")
                    .trim()
                    .to_owned();
                assert_eq!(via_policy, canonical, "{code:?} {units}");
            }
        }
    }

    proptest! {
        /// **The property that makes a display form safe.** Strip the decoration back off and
        /// the digits must parse to the exact units that went in — for every value in the
        /// domain, under a policy whose display minimum is BELOW the settlement exponent.
        ///
        /// The reverse transform here is written independently of the renderer on purpose: a
        /// round-trip through the same code proves the code is self-consistent, not correct.
        #[test]
        fn a_policy_never_changes_the_number(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let rendered = ID_IDR.render_units(units, Iso4217::IDR).unwrap();

            let bare = rendered.replace("Rp ", "");
            let (sign, digits) = match bare.strip_prefix('-') {
                Some(rest) => ("-", rest),
                None => ("", &bare[..]),
            };
            let plain = format!("{sign}{}", digits.replace('.', "").replace(',', "."));

            prop_assert_eq!(text::parse_amount(&plain).unwrap(), units);
        }

        /// Grouping inserts separators and nothing else: removing them recovers the digits the
        /// canonical form produced.
        #[test]
        fn grouping_only_ever_inserts_separators(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let grouped = EN_USD.render_units(units, Iso4217::USD).unwrap();
            let flat = grouped.replace(['$', ','], "");
            let canonical = text::render(units, Iso4217::USD).unwrap().replace("USD ", "");
            prop_assert_eq!(flat, canonical);
        }

        #[test]
        fn every_valid_grouping_policy_terminates_and_preserves_digits(
            units in -DOMAIN_MAX..=DOMAIN_MAX,
            grouping in prop::collection::vec(1u8..=8, 0..6),
            grouping_enabled in any::<bool>(),
        ) {
            let group_separator = if grouping_enabled { "," } else { "" };
            let policy = LocalePolicy::new(Iso4217::USD, "")
                .try_with_grouping(&grouping)
                .unwrap()
                .try_with_separators(group_separator, ".")
                .unwrap();

            let rendered = policy.render_units(units, Iso4217::USD).unwrap();
            let plain = rendered.replace(group_separator, "");
            prop_assert_eq!(text::parse_amount(&plain).unwrap(), units);
        }
    }
}
