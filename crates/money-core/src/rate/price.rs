//! A directed FX rate: how many `Quote` units one `Base` unit buys.

use crate::StaticCurrency;
use crate::domain::in_domain;
use crate::errors::{AmountError, RateError};
use core::marker::PhantomData;

/// A directed FX rate: how many `Quote` units one `Base` unit buys.
///
/// The value uses the crate's fixed [`SCALE`](crate::advanced::domain::SCALE).
///
/// # What this type proves
///
/// - **The pair, at compile time.** `Money<USD>` can only be converted by a `Rate<USD, IDR>`, and
///   the result can only be `Money<IDR>` — a mismatched pair does not compile.
/// - **Positivity and domain.** Rates are strictly positive and domain-bounded. Every constructor
///   and every decoding adapter enforces those runtime invariants.
/// - **One rounding per conversion.** [`Money::convert_via`](crate::Money::convert_via) rounds
///   once across both legs, so no bridge balance the holder never held is materialized.
///
/// # What this type does not prove
///
/// - **A rate is a mid-rate, not a tradeable price.** There is no bid and no ask, which is exactly
///   why `inverse()` and `compose()` are deliberately absent: inverting or composing a mid-rate
///   fabricates a price nobody can trade at. Store every pair in both directions instead.
/// - **It carries no observation time.** A `Rate` makes no claim about *when* it was true, and
///   nothing in the type prevents applying an arbitrarily stale one. Whichever service owns the
///   quote owns its freshness; that fact is temporal and belongs at run time, with the owner.
/// - **Conversion loss below one canonical unit is discarded silently.** Unlike
///   [`Money::div_int`](crate::Money::div_int), conversion returns no
///   [`Residue`](crate::Residue). The discarded part is bounded below `10^-18` of a unit, but it
///   is a loss, and nothing hands it back.
pub struct Rate<Base: StaticCurrency, Quote: StaticCurrency> {
    units: i128,
    // The value is currency-agnostic, so the phantom pair carries both type parameters.
    _pair: PhantomData<(Base, Quote)>,
}

// Manual impls avoid unnecessary `Clone`/`Copy` bounds on the phantom parameters.
impl<Base: StaticCurrency, Quote: StaticCurrency> Clone for Rate<Base, Quote> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Base: StaticCurrency, Quote: StaticCurrency> Copy for Rate<Base, Quote> {}
impl<Base: StaticCurrency, Quote: StaticCurrency> PartialEq for Rate<Base, Quote> {
    fn eq(&self, o: &Self) -> bool {
        self.units == o.units
    }
}
impl<Base: StaticCurrency, Quote: StaticCurrency> Eq for Rate<Base, Quote> {}
impl<Base: StaticCurrency, Quote: StaticCurrency> core::fmt::Debug for Rate<Base, Quote> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Rate({} units, {}->{})", self.units, Base::CODE.alpha3(), Quote::CODE.alpha3())
    }
}

impl<Base: StaticCurrency, Quote: StaticCurrency> Rate<Base, Quote> {
    /// Construct from canonical units, reporting **why** a value was refused.
    ///
    /// Every text, serde, PostgreSQL, and sqlx ingress reaches this invariant owner.
    ///
    /// # Errors
    /// [`RateError::Amount`] if the magnitude leaves the domain, and
    /// [`RateError::NonPositive`] if `units <= 0`. The domain is tested **first**, so
    /// `i128::MIN` is reported as the magnitude bug it is rather than as a sign bug, while an
    /// in-domain `-2` is reported as the sign bug it is.
    #[inline]
    pub const fn try_from_units(units: i128) -> Result<Self, RateError> {
        if !in_domain(units) {
            return Err(RateError::Amount(AmountError::out_of_domain(units)));
        }
        if units <= 0 {
            return Err(RateError::NonPositive { attempted_units: units });
        }
        Ok(Self { units, _pair: PhantomData })
    }

    /// The canonical units. Read-only: reconstructing requires a checked constructor.
    #[inline]
    #[must_use]
    pub const fn units(&self) -> i128 {
        self.units
    }
}

#[cfg(test)]
// The serde cases declare their fixture type beside the assertion it serves rather than at the
// top of a module where nothing else uses it.
#[allow(clippy::items_after_statements)]
mod tests {
    use crate::Money;
    use crate::Rate;
    use crate::domain::{DOMAIN_MAX, POW10_SCALE};
    use crate::errors::{AmountError, RateError};
    use crate::iso::{IDR, USD};
    use core::str::FromStr;

    #[test]
    fn a_zero_or_negative_rate_is_refused_at_construction() {
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(0),
            Err(RateError::NonPositive { attempted_units: 0 }),
            "a zero rate would send the money to zero, silently and with no residue"
        );
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(minus_two()),
            Err(RateError::NonPositive { attempted_units: minus_two() }),
            "a negative rate would flip the sign of the money passing through it"
        );

        assert!(Rate::<USD, IDR>::try_from_units(-1).is_err());

        // Positive controls prevent a reject-everything implementation from passing.
        assert!(Rate::<USD, IDR>::try_from_units(1).is_ok(), "1e-18 is positive and in domain");
        assert!(Rate::<USD, IDR>::try_from_units(2 * POW10_SCALE).is_ok());
    }

    /// Magnitude and sign failures remain distinguishable. Domain is tested first, so
    /// `i128::MIN` reports its magnitude failure.
    #[test]
    fn the_two_rate_refusals_are_reported_separately() {
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(i128::MIN),
            Err(RateError::Amount(AmountError::out_of_domain(i128::MIN))),
            "out of domain AND negative: the magnitude is the useful fact"
        );
        assert_eq!(
            Rate::<USD, IDR>::try_from_units(-DOMAIN_MAX),
            Err(RateError::NonPositive { attempted_units: -DOMAIN_MAX }),
            "in domain, so the sign is the only thing wrong with it"
        );
    }

    /// The magnitude bound is `Money`'s, unchanged. Only the sign rule differs between the two
    /// types, and it differs because only one of them is a price.
    #[test]
    fn rate_construction_enforces_the_same_magnitude_bound_as_money() {
        assert!(Rate::<USD, IDR>::try_from_units(DOMAIN_MAX).is_ok());
        assert!(Rate::<USD, IDR>::try_from_units(DOMAIN_MAX + 1).is_err());
        assert!(Rate::<USD, IDR>::try_from_units(i128::MIN).is_err(), "i128::MIN must not sneak in");
        // The upper bound is shared with `Money`; the lower bound is not, and this is the pair
        // that says so.
        assert!(Money::<USD>::try_from_units(-DOMAIN_MAX).is_ok(), "money is signed");
        assert!(Rate::<USD, IDR>::try_from_units(-DOMAIN_MAX).is_err(), "a rate is not");
    }

    /// `-2.0` as canonical units.
    fn minus_two() -> i128 {
        -2 * POW10_SCALE
    }

    #[test]
    fn the_text_parser_refuses_a_non_positive_quote() {
        assert_eq!(
            Rate::<USD, IDR>::from_str("USD/IDR/0"),
            Err(RateError::NonPositive { attempted_units: 0 }),
            "a zero quote is not a malformed literal -- it parses, and then it is refused"
        );
        assert_eq!(
            Rate::<USD, IDR>::from_str("USD/IDR/-2"),
            Err(RateError::NonPositive { attempted_units: minus_two() }),
        );
        // `-0` is a distinct spelling that reaches the same units, and a parser that special-cased a
        // leading `-` rather than testing the value would let it through.
        assert_eq!(
            Rate::<USD, IDR>::from_str("USD/IDR/-0.000000000000000000"),
            Err(RateError::NonPositive { attempted_units: 0 }),
        );

        assert!(Rate::<USD, IDR>::from_str("USD/IDR/16000").is_ok());
    }

    /// The parser preserves the sign error instead of reporting a domain error.
    #[test]
    fn the_parser_names_the_sign_rather_than_blaming_the_domain() {
        let err = Rate::<USD, IDR>::from_str("USD/IDR/-2").unwrap_err();
        assert!(!matches!(err, RateError::Amount(_)), "-2 is comfortably in domain; only its sign is wrong");
        let rendered = err.to_string();
        assert!(
            rendered.contains("strictly positive"),
            "the message must say what the rule is, got: {rendered}"
        );
    }

    #[cfg(feature = "serde")]
    mod wire {
        use super::minus_two;
        use crate::Rate;
        use crate::iso::{IDR, Iso4217, USD};
        use serde::Deserialize;

        /// Structured and transparent JSON reject the same invalid value.
        #[test]
        fn json_refuses_a_non_positive_quote_in_both_shapes() {
            let structured =
                serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"USD","quote":"IDR","rate":"-2"}"#);
            let err = structured.expect_err("a negative quote must not deserialise");
            assert!(
                err.to_string().contains("strictly positive"),
                "the reason must survive onto the wire error, got: {err}"
            );

            assert!(
                serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"USD","quote":"IDR","rate":"0"}"#).is_err(),
                "a zero quote must not deserialise either"
            );

            #[derive(Deserialize)]
            struct T(#[serde(with = "crate::wire::transparent")] Rate<USD, IDR>);
            assert!(
                serde_json::from_str::<T>(r#""USD/IDR/-2""#).is_err(),
                "the transparent form is a separate code path and must refuse it too"
            );
            // Decode a positive value through the same transparent shape.
            let accepted: T = serde_json::from_str(r#""USD/IDR/16000""#).unwrap();
            assert_eq!(
                accepted.0,
                Rate::<USD, IDR>::try_from_units(16_000 * crate::domain::POW10_SCALE).unwrap()
            );

            // The positive control, so this cannot pass by refusing everything.
            assert!(
                serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"USD","quote":"IDR","rate":"16000"}"#)
                    .is_ok()
            );
        }

        /// Binary decoding validates its direct `(ISO numeric, ISO numeric, i128)` form.
        #[test]
        fn binary_refuses_a_non_positive_quote() {
            let bytes = postcard::to_allocvec(&(Iso4217::USD, Iso4217::IDR, minus_two())).unwrap();
            assert!(
                postcard::from_bytes::<Rate<USD, IDR>>(&bytes).is_err(),
                "the binary form skips the text parser, so it needs its own proof"
            );

            let zero = postcard::to_allocvec(&(Iso4217::USD, Iso4217::IDR, 0i128)).unwrap();
            assert!(postcard::from_bytes::<Rate<USD, IDR>>(&zero).is_err());

            // Positive control also pins the tuple shape used above.
            let good =
                postcard::to_allocvec(&(Iso4217::USD, Iso4217::IDR, 16_000 * crate::domain::POW10_SCALE))
                    .unwrap();
            assert_eq!(
                postcard::from_bytes::<Rate<USD, IDR>>(&good).unwrap(),
                Rate::<USD, IDR>::try_from_units(16_000 * crate::domain::POW10_SCALE).unwrap()
            );
        }
    }

    /// Exercise `FromSql` directly with its byte and OID inputs.
    #[cfg(feature = "postgres")]
    #[test]
    fn postgres_types_refuses_a_non_positive_quote() {
        use postgres::types::{FromSql, Type};

        let err = <Rate<USD, IDR> as FromSql>::from_sql(&Type::TEXT, b"USD/IDR/-2")
            .expect_err("a negative quote must not decode out of a column");
        assert!(
            err.to_string().contains("strictly positive"),
            "the reason must survive the adapter, got: {err}"
        );

        assert!(<Rate<USD, IDR> as FromSql>::from_sql(&Type::TEXT, b"USD/IDR/0").is_err());
        assert!(<Rate<USD, IDR> as FromSql>::from_sql(&Type::TEXT, b"USD/IDR/16000").is_ok());
    }
}
