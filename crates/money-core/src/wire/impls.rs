//! The default structured form, and the currency cross-check every decode performs.

use super::{
    money_from_amount, money_from_binary, money_to_binary, rate_from_amount, rate_from_binary,
    rate_to_binary, to_de_error,
};
use crate::errors::{CurrencyMismatch, WireError};
use crate::iso::Iso4217;
use crate::text::{render_amount, render_rate};
use crate::{Money, Rate, StaticCurrency};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

#[derive(Serialize)]
struct MoneyOut<'a> {
    currency: Iso4217,
    amount: &'a str,
}

#[derive(Deserialize)]
struct MoneyIn<'a> {
    currency: Iso4217,
    #[serde(borrow)]
    amount: Cow<'a, str>,
}

#[derive(Serialize)]
struct RateOut<'a> {
    base: Iso4217,
    quote: Iso4217,
    rate: &'a str,
}

#[derive(Deserialize)]
struct RateIn<'a> {
    base: Iso4217,
    quote: Iso4217,
    #[serde(borrow)]
    rate: Cow<'a, str>,
}

impl<C: StaticCurrency> Serialize for Money<C> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            MoneyOut { currency: C::CODE, amount: &render_amount(*self) }.serialize(s)
        } else {
            money_to_binary(*self, s)
        }
    }
}

impl<'de, C: StaticCurrency> Deserialize<'de> for Money<C> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if !d.is_human_readable() {
            return money_from_binary(d);
        }
        let raw = MoneyIn::deserialize(d)?;
        // The redundancy is the point: it catches an IDR amount in a USD field.
        if raw.currency != C::CODE {
            return Err(to_de_error(&WireError::WrongCurrency(CurrencyMismatch {
                expected: C::CODE,
                found: raw.currency,
            })));
        }
        // Parse the amount field directly. Reconstructing a tagged string here
        // allocated and then made the text parser split a tag already checked
        // above.
        money_from_amount(raw.amount.as_ref()).map_err(|e| to_de_error(&e))
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Serialize for Rate<Base, Quote> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            RateOut { base: Base::CODE, quote: Quote::CODE, rate: &render_rate(*self) }.serialize(s)
        } else {
            rate_to_binary(*self, s)
        }
    }
}

impl<'de, Base: StaticCurrency, Quote: StaticCurrency> Deserialize<'de> for Rate<Base, Quote> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if !d.is_human_readable() {
            return rate_from_binary(d);
        }
        let raw = RateIn::deserialize(d)?;
        if raw.base != Base::CODE {
            return Err(to_de_error(&WireError::WrongCurrency(CurrencyMismatch {
                expected: Base::CODE,
                found: raw.base,
            })));
        }
        if raw.quote != Quote::CODE {
            return Err(to_de_error(&WireError::WrongCurrency(CurrencyMismatch {
                expected: Quote::CODE,
                found: raw.quote,
            })));
        }
        rate_from_amount(raw.rate.as_ref()).map_err(|e| to_de_error(&e))
    }
}

#[cfg(test)]
// The serde cases declare their fixture type beside the assertion it serves rather than at the
// top of a module where nothing else uses it.
#[allow(clippy::items_after_statements)]
mod tests {
    use super::{MoneyIn, RateIn};
    use crate::domain::DOMAIN_MAX;
    use crate::iso::{IDR, Iso4217, JPY, USD};
    use crate::{Money, Rate};
    use serde::{Deserialize, Serialize};

    use proptest::prelude::*;
    use std::borrow::Cow;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Payment {
        amount: Money<USD>, // structured, the default
        #[serde(with = "crate::wire::transparent")]
        fee: Money<USD>,
        #[serde(with = "crate::wire::transparent")]
        rate: Rate<USD, IDR>,
    }

    #[test]
    fn a_struct_can_mix_both_modes_per_field() {
        let p = Payment {
            amount: Money::<USD>::try_from_major(10).unwrap(),
            fee: Money::<USD>::try_from_units(1_500_000_000_000_000_000).unwrap(),
            rate: Rate::<USD, IDR>::try_from_units(16_000 * crate::domain::POW10_SCALE).unwrap(),
        };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(
            json,
            r#"{"amount":{"currency":"USD","amount":"10.00"},"fee":"USD 1.50","rate":"USD/IDR/16000"}"#
        );
        assert_eq!(serde_json::from_str::<Payment>(&json).unwrap(), p);
    }
    #[test]
    fn structured_is_the_default_for_money_and_rate() {
        let m = Money::<USD>::try_from_units(10_500_000_000_000_000_000).unwrap();
        assert_eq!(serde_json::to_string(&m).unwrap(), r#"{"currency":"USD","amount":"10.50"}"#);

        let r = Rate::<USD, IDR>::try_from_units(16_000 * crate::domain::POW10_SCALE).unwrap();
        assert_eq!(serde_json::to_string(&r).unwrap(), r#"{"base":"USD","quote":"IDR","rate":"16000"}"#);
    }
    /// The amount field follows the same trim rule as `Display`: minimum is the currency's ISO
    /// settlement exponent. One rule, one implementation, no chance of the two disagreeing.
    #[test]
    fn the_wire_amount_uses_the_same_trim_rule_as_display() {
        let units = 10_500_000_000_000_000_000;
        assert_eq!(
            serde_json::to_string(&Money::<JPY>::try_from_units(units).unwrap()).unwrap(),
            r#"{"currency":"JPY","amount":"10.5"}"#,
            "JPY settles at 0dp"
        );
        assert_eq!(
            serde_json::to_string(&Money::<USD>::try_from_units(units).unwrap()).unwrap(),
            r#"{"currency":"USD","amount":"10.50"}"#,
            "USD settles at 2dp"
        );
    }
    // ---------------------------------------------------------------------------------------
    // The cross-check
    // ---------------------------------------------------------------------------------------

    /// Deserializing `Money<USD>` from an IDR payload is an ERROR, in both modes.
    ///
    /// The currency in the payload is redundant with the field's type ON PURPOSE: it catches an
    /// IDR value landing in a USD field at an API boundary, which is exactly where types cannot
    /// help.
    #[test]
    fn the_currency_cross_check_fires_in_both_modes() {
        assert!(
            serde_json::from_str::<Money<USD>>(r#"{"currency":"IDR","amount":"10.50"}"#).is_err(),
            "structured must reject a mismatched currency"
        );

        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(with = "crate::wire::transparent")]
            #[allow(dead_code)]
            m: Money<USD>,
        }
        assert!(
            serde_json::from_str::<Wrapper>(r#"{"m":"IDR 10.50"}"#).is_err(),
            "transparent must reject a mismatched currency"
        );
    }
    #[test]
    fn a_rate_checks_both_ends_of_the_pair_on_the_wire() {
        assert!(
            serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"JPY","quote":"IDR","rate":"1"}"#).is_err(),
            "the base end must be checked"
        );
        assert!(
            serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"USD","quote":"JPY","rate":"1"}"#).is_err(),
            "the quote end must be checked"
        );
    }
    #[test]
    fn out_of_domain_and_over_precise_payloads_are_refused_not_rounded() {
        assert!(
            serde_json::from_str::<Money<USD>>(r#"{"currency":"USD","amount":"0.0000000000000000005"}"#)
                .is_err(),
            "19dp must be refused, never rounded"
        );
        assert!(
            serde_json::from_str::<Money<USD>>(r#"{"currency":"USD","amount":"1000000000000000000.00"}"#)
                .is_err(),
            "one major unit past the domain"
        );
    }
    // ---------------------------------------------------------------------------------------
    // Binary
    // ---------------------------------------------------------------------------------------

    /// Binary carries the currency as its ISO **numeric** code, ahead of the units — the same
    /// stable tag the human-readable form carries as alpha-3.
    ///
    /// A bare `i128` carries no identity and could be decoded under a different currency type.
    #[test]
    fn binary_carries_the_iso_numeric_tag_before_the_units() {
        let units = 10_500_000_000_000_000_000i128;
        let bytes = postcard::to_allocvec(&Money::<USD>::try_from_units(units).unwrap()).unwrap();

        // The tag is exactly what the standalone `Iso4217` codec emits (USD = numeric 840), never
        // the enum ordinal — so it inherits `binary_encodes_the_iso_numeric_never_the_variant_position`.
        let expected = postcard::to_allocvec(&(Iso4217::USD, units)).unwrap();
        assert_eq!(bytes, expected, "binary is (ISO numeric, i128 units)");

        // The currency tag makes this distinct from a bare amount.
        let bare = postcard::to_allocvec(&units).unwrap();
        assert_ne!(bytes, bare, "the currency must now be on the wire");
    }
    /// A `Money<USD>` payload must not decode as `Money<IDR>`.
    #[test]
    fn binary_refuses_a_cross_currency_reinterpretation() {
        let m = Money::<USD>::try_from_units(10 * crate::domain::POW10_SCALE).unwrap();
        let bytes = postcard::to_allocvec(&m).unwrap();

        assert!(postcard::from_bytes::<Money<IDR>>(&bytes).is_err(), "a USD payload must not decode as IDR");
        // ...while still round-tripping into its own type, in both binary modes.
        assert_eq!(postcard::from_bytes::<Money<USD>>(&bytes).unwrap(), m);
    }
    /// A `Rate` tags **both** ends. Swapping either the base or the quote type must be refused, not
    /// silently reinterpreted — the pair identity is exactly what a refactor is most likely to move.
    #[test]
    fn binary_refuses_a_rate_pair_reinterpretation() {
        use crate::iso::{EUR, JPY};
        let r = Rate::<USD, IDR>::try_from_units(16_000 * crate::domain::POW10_SCALE).unwrap();
        let bytes = postcard::to_allocvec(&r).unwrap();

        assert!(postcard::from_bytes::<Rate<EUR, JPY>>(&bytes).is_err(), "both ends changed");
        assert!(postcard::from_bytes::<Rate<JPY, IDR>>(&bytes).is_err(), "the base end changed");
        assert!(postcard::from_bytes::<Rate<USD, JPY>>(&bytes).is_err(), "the quote end changed");
        assert_eq!(postcard::from_bytes::<Rate<USD, IDR>>(&bytes).unwrap(), r);
    }
    #[test]
    fn binary_round_trips_in_both_modes() {
        let m = Money::<USD>::try_from_units(-10_500_000_000_000_000_000).unwrap();
        let bytes = postcard::to_allocvec(&m).unwrap();
        assert_eq!(postcard::from_bytes::<Money<USD>>(&bytes).unwrap(), m);

        let r = Rate::<USD, IDR>::try_from_units(DOMAIN_MAX).unwrap();
        let bytes = postcard::to_allocvec(&r).unwrap();
        assert_eq!(postcard::from_bytes::<Rate<USD, IDR>>(&bytes).unwrap(), r);
    }
    #[test]
    fn a_binary_payload_outside_the_domain_is_refused() {
        // A well-formed tag (USD) with an out-of-domain units field: the domain check must still
        // fire after the currency check passes.
        let bytes = postcard::to_allocvec(&(Iso4217::USD, DOMAIN_MAX + 1)).unwrap();
        assert!(postcard::from_bytes::<Money<USD>>(&bytes).is_err());
    }
    proptest! {
        #[test]
        fn prop_money_round_trips_through_json(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let m = Money::<USD>::try_from_units(units).unwrap();
            let json = serde_json::to_string(&m).unwrap();
            prop_assert_eq!(serde_json::from_str::<Money<USD>>(&json).unwrap(), m);
        }

        #[test]
        fn prop_money_round_trips_through_binary(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            let m = Money::<IDR>::try_from_units(units).unwrap();
            let bytes = postcard::to_allocvec(&m).unwrap();
            prop_assert_eq!(postcard::from_bytes::<Money<IDR>>(&bytes).unwrap(), m);
        }

        /// Both human-readable modes carry the same VALUE, whatever their shape.
        #[test]
        fn prop_transparent_and_structured_agree_on_the_value(units in -DOMAIN_MAX..=DOMAIN_MAX) {
            #[derive(Serialize, Deserialize, PartialEq, Debug)]
            struct T(#[serde(with = "crate::wire::transparent")] Money<USD>);

            let m = Money::<USD>::try_from_units(units).unwrap();
            let via_structured: Money<USD> =
                serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
            let via_transparent: T =
                serde_json::from_str(&serde_json::to_string(&T(m)).unwrap()).unwrap();
            prop_assert_eq!(via_structured, via_transparent.0);
            prop_assert_eq!(via_structured, m);
        }

        /// Non-positive ingress cases live in `rate_ingress.rs`; this property covers valid values.
        #[test]
        fn prop_rate_round_trips_through_both_shapes(units in 1..=DOMAIN_MAX) {
            #[derive(Serialize, Deserialize, PartialEq, Debug)]
            struct T(#[serde(with = "crate::wire::transparent")] Rate<USD, IDR>);

            let r = Rate::<USD, IDR>::try_from_units(units).unwrap();
            let structured: Rate<USD, IDR> =
                serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
            let transparent: T = serde_json::from_str(&serde_json::to_string(&T(r)).unwrap()).unwrap();
            prop_assert_eq!(structured, r);
            prop_assert_eq!(transparent.0, r);
        }
    }

    #[test]
    fn structured_numbers_borrow_when_the_input_needs_no_unescaping() {
        let money: MoneyIn<'_> = serde_json::from_str(r#"{"currency":"USD","amount":"10.50"}"#).unwrap();
        let rate: RateIn<'_> =
            serde_json::from_str(r#"{"base":"USD","quote":"IDR","rate":"16000"}"#).unwrap();

        assert!(matches!(money.amount, Cow::Borrowed("10.50")));
        assert!(matches!(rate.rate, Cow::Borrowed("16000")));
    }
}
