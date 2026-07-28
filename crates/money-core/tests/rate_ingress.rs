//! Every public way to build a `Rate`, shown to refuse a non-positive one. (H1; DESIGN.md C6)
//!
//! # Why a whole file, when one constructor owns the rule
//!
//! Because "one constructor owns the rule" is an argument, and this file is the evidence. Five
//! public surfaces can hand this crate a rate: the raw constructor, the text parser, serde's two
//! wire forms, `postgres-types`, and sqlx. They are meant to funnel through
//! [`Rate::try_from_units`], so tightening that one function tightens all of them at once — but
//! an adapter that quietly grew its own parse would enforce a weaker rule and nothing else in
//! the tree would notice. The claim being tested is therefore not "the constructor refuses zero"
//! (`rate.rs` pins that) but **"no ingress is weaker than the constructor"**.
//!
//! That distinction earned its own file on 2026-07-27. Until then `Rate` accepted the full signed
//! domain deliberately — C6 bounds magnitude and is silent on sign — on the reasoning that sign
//! is a quote feed's responsibility. The decision recorded one condition for revisiting itself:
//! *"if a feed is ever ingested without validation."* Four of the five surfaces below **are** feed
//! ingress, shipped in this repository, decoding untrusted bytes with no positivity check of their
//! own. The condition was met by this crate's own code, which is why the decision was re-taken.
//!
//! # What a zero or negative rate does if it gets through
//!
//! Nothing loud. A zero rate sends the converted amount to zero; a negative one reverses its sign.
//! Both are ordinary arithmetic on in-domain values — no overflow, no residue, no error, and a
//! perfectly plausible number on the other side. That is the whole reason this is enforced at
//! construction rather than checked by the caller who remembers to.
//!
//! # The fifth ingress
//!
//! sqlx's `Decode` is asserted in `sqlx_roundtrip.rs` instead of here, because proving it offline
//! would mean hand-building a `PgValueRef` and proving the wrong thing. It decodes a real
//! non-positive quote off a real server there.

use core::str::FromStr;
use kamu_money_core::iso::{IDR, USD};
use kamu_money_core::rate::Rate;
use kamu_money_core::{POW10_SCALE, RateError};

/// `-2.0`, as canonical units. A plain, plausible-looking quote — not an edge case, which is
/// the point: the dangerous input here is the one that looks like a price.
fn minus_two() -> i128 {
    -2 * POW10_SCALE
}

// --- ingress 1: the raw constructor ----------------------------------------------------------

#[test]
fn the_raw_constructor_refuses_zero_and_negatives() {
    assert_eq!(Rate::<USD, IDR>::try_from_units(0), Err(RateError::NonPositive { attempted_units: 0 }));
    assert_eq!(
        Rate::<USD, IDR>::try_from_units(minus_two()),
        Err(RateError::NonPositive { attempted_units: minus_two() })
    );
    assert!(Rate::<USD, IDR>::try_from_units(0).is_err());
    assert!(Rate::<USD, IDR>::try_from_units(minus_two()).is_err());

    // A positive rate still constructs, including the smallest representable one. Without this
    // the three assertions above are also satisfied by a constructor that refuses everything.
    assert!(Rate::<USD, IDR>::try_from_units(1).is_ok());
    assert!(Rate::<USD, IDR>::try_from_units(2 * POW10_SCALE).is_ok());
}

// --- ingress 2: the text parser --------------------------------------------------------------

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

/// The parser reports the SIGN, not a domain overflow — which is what it would have reported had
/// `from_str` once collapsed every constructor refusal into a domain overflow. At a feed boundary the error
/// string is the thing a human reads to find out what the counterparty sent, so naming the wrong
/// defect there costs an investigation.
#[test]
fn the_parser_names_the_sign_rather_than_blaming_the_domain() {
    let err = Rate::<USD, IDR>::from_str("USD/IDR/-2").unwrap_err();
    assert!(!matches!(err, RateError::Amount(_)), "-2 is comfortably in domain; only its sign is wrong");
    let rendered = err.to_string();
    assert!(rendered.contains("strictly positive"), "the message must say what the rule is, got: {rendered}");
}

// --- ingress 3 and 4: serde, both wire forms -------------------------------------------------

#[cfg(feature = "serde")]
mod wire {
    use super::minus_two;
    use kamu_money_core::iso::{IDR, Iso4217, USD};
    use kamu_money_core::rate::Rate;
    use serde::Deserialize;

    /// The structured JSON form, and the transparent one beside it. Both are public API and a
    /// caller picks per field, so proving one proves nothing about the other.
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
        // ...and still accepts a real quote. This also reads `T.0`, which is what proves the
        // refusal above came from the value rather than from a shape that never decodes.
        let accepted: T = serde_json::from_str(r#""USD/IDR/16000""#).unwrap();
        assert_eq!(
            accepted.0,
            Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::POW10_SCALE).unwrap()
        );

        // The positive control, so this cannot pass by refusing everything.
        assert!(
            serde_json::from_str::<Rate<USD, IDR>>(r#"{"base":"USD","quote":"IDR","rate":"16000"}"#).is_ok()
        );
    }

    /// Binary does NOT reuse the text parser — it decodes `(ISO numeric, ISO numeric, i128)`
    /// straight to units (R2-F2) — so it is the ingress most likely to drift from the rule. The
    /// bytes are built the same way `wire.rs`'s own tests build them: by encoding the tuple the
    /// codec is specified as, rather than by copying a literal nobody can re-derive.
    #[test]
    fn binary_refuses_a_non_positive_quote() {
        let bytes = postcard::to_allocvec(&(Iso4217::USD, Iso4217::IDR, minus_two())).unwrap();
        assert!(
            postcard::from_bytes::<Rate<USD, IDR>>(&bytes).is_err(),
            "the binary form skips the text parser, so it needs its own proof"
        );

        let zero = postcard::to_allocvec(&(Iso4217::USD, Iso4217::IDR, 0i128)).unwrap();
        assert!(postcard::from_bytes::<Rate<USD, IDR>>(&zero).is_err());

        // Positive control, and it doubles as a check that the tuple shape above is still the
        // codec's shape: if it were not, the negative cases would be failing to decode for the
        // wrong reason and would keep passing forever.
        let good =
            postcard::to_allocvec(&(Iso4217::USD, Iso4217::IDR, 16_000 * kamu_money_core::POW10_SCALE))
                .unwrap();
        assert_eq!(
            postcard::from_bytes::<Rate<USD, IDR>>(&good).unwrap(),
            Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::POW10_SCALE).unwrap()
        );
    }
}

// --- ingress 5: postgres-types ---------------------------------------------------------------

/// `FromSql` needs no server: it turns bytes and an OID into a value, and that is the whole
/// surface. Running it directly is a stronger test than a round trip through a container would
/// be, because it cannot accidentally pass by never reaching the decoder.
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
