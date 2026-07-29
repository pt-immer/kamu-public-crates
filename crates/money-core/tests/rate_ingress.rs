//! Contract tests for non-positive `Rate` values at every public ingress.
//!
//! Raw construction, text, serde, and `postgres-types` must preserve
//! [`Rate::try_from_units`]'s positivity rule. `sqlx_roundtrip.rs` checks sqlx
//! against a real server because an artificial `PgValueRef` would not prove the
//! adapter path.

use core::str::FromStr;
use kamu_money_core::Rate;
use kamu_money_core::advanced::domain::POW10_SCALE;
use kamu_money_core::errors::RateError;
use kamu_money_core::iso::{IDR, USD};

/// `-2.0` as canonical units.
fn minus_two() -> i128 {
    -2 * POW10_SCALE
}

#[test]
fn the_raw_constructor_refuses_zero_and_negatives() {
    assert_eq!(Rate::<USD, IDR>::try_from_units(0), Err(RateError::NonPositive { attempted_units: 0 }));
    assert_eq!(
        Rate::<USD, IDR>::try_from_units(minus_two()),
        Err(RateError::NonPositive { attempted_units: minus_two() })
    );
    // Positive controls prevent a reject-everything implementation from passing.
    assert!(Rate::<USD, IDR>::try_from_units(1).is_ok());
    assert!(Rate::<USD, IDR>::try_from_units(2 * POW10_SCALE).is_ok());
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
    assert!(rendered.contains("strictly positive"), "the message must say what the rule is, got: {rendered}");
}

#[cfg(feature = "serde")]
mod wire {
    use super::minus_two;
    use kamu_money_core::Rate;
    use kamu_money_core::iso::{IDR, Iso4217, USD};
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
        struct T(#[serde(with = "kamu_money_core::wire::transparent")] Rate<USD, IDR>);
        assert!(
            serde_json::from_str::<T>(r#""USD/IDR/-2""#).is_err(),
            "the transparent form is a separate code path and must refuse it too"
        );
        // Decode a positive value through the same transparent shape.
        let accepted: T = serde_json::from_str(r#""USD/IDR/16000""#).unwrap();
        assert_eq!(
            accepted.0,
            Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::advanced::domain::POW10_SCALE)
                .unwrap()
        );

        // The positive control, so this cannot pass by refusing everything.
        assert!(
            serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"USD","quote":"IDR","rate":"16000"}"#).is_ok()
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
        let good = postcard::to_allocvec(&(
            Iso4217::USD,
            Iso4217::IDR,
            16_000 * kamu_money_core::advanced::domain::POW10_SCALE,
        ))
        .unwrap();
        assert_eq!(
            postcard::from_bytes::<Rate<USD, IDR>>(&good).unwrap(),
            Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::advanced::domain::POW10_SCALE)
                .unwrap()
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
    assert!(err.to_string().contains("strictly positive"), "the reason must survive the adapter, got: {err}");

    assert!(<Rate<USD, IDR> as FromSql>::from_sql(&Type::TEXT, b"USD/IDR/0").is_err());
    assert!(<Rate<USD, IDR> as FromSql>::from_sql(&Type::TEXT, b"USD/IDR/16000").is_ok());
}
