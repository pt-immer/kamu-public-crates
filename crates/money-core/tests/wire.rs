//! The serde wire. (DESIGN.md C7)
//!
//! Runs only with `--features serde`. `cargo test --workspace --all-features` covers it.

#![cfg(feature = "serde")]

use kamu_money_core::Money;
use kamu_money_core::Rate;
use kamu_money_core::advanced::domain::DOMAIN_MAX;
use kamu_money_core::iso::{IDR, Iso4217, JPY, USD};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------
// THE measured trap: serde encodes an enum variant by POSITION, not by discriminant.
// ---------------------------------------------------------------------------------------

/// Binary must carry the ISO **numeric** code, which a standards body assigns permanently —
/// never the variant's ordinal position, which moves the moment a currency is inserted
/// mid-table. The register is complete at 178 codes and still grows — ISO issues them — and
/// because variants are emitted in alpha-3 order, a new code lands between existing ones rather
/// than after them. (This comment previously justified the risk with "12 of ~180 and WILL
/// grow", which stopped being true when the table was generated from the published list.)
///
/// Measured previously with a derived impl: after inserting one currency, stored `IDR` decoded
/// as `GBP`, silently, with `#[repr(u16)]` and `IDR = 360` unchanged in both versions.
///
/// A JSON suite cannot catch this — human-readable formats emit the NAME. That is why this
/// test is binary, and why it is the most important one in the file.
#[test]
fn binary_encodes_the_iso_numeric_never_the_variant_position() {
    let encoded = postcard::to_allocvec(&Iso4217::IDR).unwrap();

    assert_eq!(encoded, postcard::to_allocvec(&360u16).unwrap(), "IDR must encode as its ISO numeric 360");
    // IDR is the SECOND variant in the table, so a position-based encoding would emit 1.
    assert_ne!(
        encoded,
        postcard::to_allocvec(&1u16).unwrap(),
        "must not be the ordinal position — that is the silent-corruption bug"
    );
    assert_eq!(postcard::from_bytes::<Iso4217>(&encoded).unwrap(), Iso4217::IDR);
}

#[test]
fn human_readable_uses_the_alpha3_code_with_no_rename_all_mangling() {
    // `rename_all = "SCREAMING_SNAKE_CASE"` would emit "I_D_R" here — measured, and it reads
    // MORE correct in the source than it behaves.
    assert_eq!(serde_json::to_string(&Iso4217::IDR).unwrap(), r#""IDR""#);
    assert_eq!(serde_json::from_str::<Iso4217>(r#""IDR""#).unwrap(), Iso4217::IDR);
    assert!(serde_json::from_str::<Iso4217>(r#""I_D_R""#).is_err());
    assert!(serde_json::from_str::<Iso4217>(r#""ZZZ""#).is_err());
}

// ---------------------------------------------------------------------------------------
// The two modes
// ---------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Payment {
    amount: Money<USD>, // structured, the default
    #[serde(with = "kamu_money_core::wire::transparent")]
    fee: Money<USD>,
    #[serde(with = "kamu_money_core::wire::transparent")]
    rate: Rate<USD, IDR>,
}

#[test]
fn a_struct_can_mix_both_modes_per_field() {
    let p = Payment {
        amount: Money::<USD>::try_from_major(10).unwrap(),
        fee: Money::<USD>::try_from_units(1_500_000_000_000_000_000).unwrap(),
        rate: Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::advanced::domain::POW10_SCALE)
            .unwrap(),
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

    let r =
        Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::advanced::domain::POW10_SCALE).unwrap();
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
        #[serde(with = "kamu_money_core::wire::transparent")]
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
        serde_json::from_str::<Money<USD>>(r#"{"currency":"USD","amount":"0.0000000000000000005"}"#).is_err(),
        "19dp must be refused, never rounded — this is the rust_decimal failure (E2)"
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
/// This replaces a test that asserted the opposite: that the binary encoding was byte-identical
/// to a bare `i128`, "the currency costs zero bytes". That elegance was the R2-F2 defect — a
/// bare `i128` carries no identity, so `Money<USD>` bytes decoded as `Money<IDR>` with the units
/// preserved and the currency silently reassigned. A number without its currency is not money,
/// which is this crate's whole thesis; the wire may not spend the one thing it refuses to.
#[test]
fn binary_carries_the_iso_numeric_tag_before_the_units() {
    let units = 10_500_000_000_000_000_000i128;
    let bytes = postcard::to_allocvec(&Money::<USD>::try_from_units(units).unwrap()).unwrap();

    // The tag is exactly what the standalone `Iso4217` codec emits (USD = numeric 840), never
    // the enum ordinal — so it inherits `binary_encodes_the_iso_numeric_never_the_variant_position`.
    let expected = postcard::to_allocvec(&(Iso4217::USD, units)).unwrap();
    assert_eq!(bytes, expected, "binary is (ISO numeric, i128 units)");

    // And it must NOT be the bare `i128` that reinterpreted silently before R2-F2.
    let bare = postcard::to_allocvec(&units).unwrap();
    assert_ne!(bytes, bare, "the currency must now be on the wire");
}

/// The defect itself, as a regression guard: a `Money<USD>` payload must not decode as
/// `Money<IDR>`. Before R2-F2 this succeeded, unit-for-unit, silently redenominating the money.
#[test]
fn binary_refuses_a_cross_currency_reinterpretation() {
    let m = Money::<USD>::try_from_units(10 * kamu_money_core::advanced::domain::POW10_SCALE).unwrap();
    let bytes = postcard::to_allocvec(&m).unwrap();

    assert!(postcard::from_bytes::<Money<IDR>>(&bytes).is_err(), "a USD payload must not decode as IDR");
    // ...while still round-tripping into its own type, in both binary modes.
    assert_eq!(postcard::from_bytes::<Money<USD>>(&bytes).unwrap(), m);
}

/// A `Rate` tags **both** ends. Swapping either the base or the quote type must be refused, not
/// silently reinterpreted — the pair identity is exactly what a refactor is most likely to move.
#[test]
fn binary_refuses_a_rate_pair_reinterpretation() {
    use kamu_money_core::iso::{EUR, JPY};
    let r =
        Rate::<USD, IDR>::try_from_units(16_000 * kamu_money_core::advanced::domain::POW10_SCALE).unwrap();
    let bytes = postcard::to_allocvec(&r).unwrap();

    assert!(postcard::from_bytes::<Rate<EUR, JPY>>(&bytes).is_err(), "both ends changed");
    assert!(postcard::from_bytes::<Rate<JPY, IDR>>(&bytes).is_err(), "the base end changed");
    assert!(postcard::from_bytes::<Rate<USD, JPY>>(&bytes).is_err(), "the quote end changed");
    assert_eq!(postcard::from_bytes::<Rate<USD, IDR>>(&bytes).unwrap(), r);
}

/// The transparent mode's binary form is tagged too — otherwise `#[serde(with = transparent)]`
/// would be a silent hole in exactly the cross-check the default form now enforces.
#[test]
fn transparent_binary_also_refuses_a_cross_currency_reinterpretation() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct U(#[serde(with = "kamu_money_core::wire::transparent")] Money<USD>);
    #[derive(Deserialize)]
    struct I(
        #[serde(with = "kamu_money_core::wire::transparent")]
        #[allow(dead_code)]
        Money<IDR>,
    );

    let bytes = postcard::to_allocvec(&U(Money::<USD>::try_from_major(10).unwrap())).unwrap();
    assert!(
        postcard::from_bytes::<I>(&bytes).is_err(),
        "transparent binary must reject a mismatched currency too"
    );
    assert_eq!(postcard::from_bytes::<U>(&bytes).unwrap(), U(Money::<USD>::try_from_major(10).unwrap()));
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
        struct T(#[serde(with = "kamu_money_core::wire::transparent")] Money<USD>);

        let m = Money::<USD>::try_from_units(units).unwrap();
        let via_structured: Money<USD> =
            serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        let via_transparent: T =
            serde_json::from_str(&serde_json::to_string(&T(m)).unwrap()).unwrap();
        prop_assert_eq!(via_structured, via_transparent.0);
        prop_assert_eq!(via_structured, m);
    }

    /// Positive-only since H1 (2026-07-27). The wire is one of the four ingresses that made
    /// `Rate` enforce positivity at all; what a non-positive rate does on the way IN is
    /// asserted in `rate_ingress.rs` rather than here, where the property is round-tripping.
    #[test]
    fn prop_rate_round_trips_through_both_shapes(units in 1..=DOMAIN_MAX) {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct T(#[serde(with = "kamu_money_core::wire::transparent")] Rate<USD, IDR>);

        let r = Rate::<USD, IDR>::try_from_units(units).unwrap();
        let structured: Rate<USD, IDR> =
            serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        let transparent: T = serde_json::from_str(&serde_json::to_string(&T(r)).unwrap()).unwrap();
        prop_assert_eq!(structured, r);
        prop_assert_eq!(transparent.0, r);
    }
}
