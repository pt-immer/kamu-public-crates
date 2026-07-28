//! The canonical text form: one trim rule, shared by `Display` and (later) the serde wire.
//!
//! Render at 18dp, strip trailing zeros, **stop at the currency's ISO settlement exponent**.
//! Never round — padding is the only thing it adds, so §0.1's "display pads, never rounds"
//! holds. Parse is liberal: any exact decimal is accepted, so the round trip is a
//! **retraction** (`parse(render(v)) == v`) and not a bijection. (DESIGN.md C7)

use kamu_money_core::Money;
use kamu_money_core::advanced::domain::{DOMAIN_MAX, POW10_SCALE};
use kamu_money_core::errors::{AmountError, ParseMoneyError};
use kamu_money_core::iso::{IDR, JPY, KWD, USD, XAU};
use proptest::prelude::*;
use std::str::FromStr;

fn usd(units: i128) -> Money<USD> {
    Money::<USD>::try_from_units(units).unwrap()
}

/// The whole trim rule, as a table. One stored value, four settlement exponents.
#[test]
fn the_minimum_width_is_the_iso_settlement_exponent() {
    let half = 10_500_000_000_000_000_000; // 10.5
    assert_eq!(Money::<USD>::try_from_units(half).unwrap().to_string(), "USD 10.50"); // exp 2
    assert_eq!(Money::<JPY>::try_from_units(half).unwrap().to_string(), "JPY 10.5"); // exp 0
    assert_eq!(Money::<KWD>::try_from_units(half).unwrap().to_string(), "KWD 10.500"); // exp 3
    assert_eq!(Money::<XAU>::try_from_units(half).unwrap().to_string(), "XAU 10.5"); // None -> 0

    let whole = 10_000_000_000_000_000_000; // 10
    assert_eq!(Money::<USD>::try_from_units(whole).unwrap().to_string(), "USD 10.00");
    assert_eq!(Money::<JPY>::try_from_units(whole).unwrap().to_string(), "JPY 10");
    assert_eq!(Money::<KWD>::try_from_units(whole).unwrap().to_string(), "KWD 10.000");
    assert_eq!(Money::<XAU>::try_from_units(whole).unwrap().to_string(), "XAU 10");
}

/// Every significant digit survives, all the way down to one canonical unit.
///
/// This is the half of §0.1 that trimming could have broken: padding up to the settlement
/// exponent is fine, but nothing may ever be dropped off the right-hand end.
#[test]
fn trimming_never_rounds() {
    assert_eq!(usd(10_123_456_789_000_000_000).to_string(), "USD 10.123456789");
    assert_eq!(usd(1).to_string(), "USD 0.000000000000000001");
    assert_eq!(usd(-1).to_string(), "USD -0.000000000000000001");
    assert_eq!(usd(DOMAIN_MAX).to_string(), "USD 999999999999999999.999999999999999999");
}

#[test]
fn zero_and_sign_render_correctly() {
    assert_eq!(Money::<USD>::ZERO.to_string(), "USD 0.00");
    assert_eq!(Money::<JPY>::ZERO.to_string(), "JPY 0");
    assert_eq!(usd(-10_500_000_000_000_000_000).to_string(), "USD -10.50");
    // -0 does not exist: there is one zero, and it has no sign.
    assert_eq!(usd(0).to_string(), "USD 0.00");
}

/// Parse is LIBERAL: it accepts any exact decimal, not only the canonical spelling.
///
/// That is a deliberate weakening of C7's original "bijection" claim, and it is why the
/// property below is `parse(render(v)) == v` rather than a two-way identity.
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
        Err(ParseMoneyError::WrongCurrency {
            expected: kamu_money_core::Iso4217::USD,
            found: kamu_money_core::Iso4217::IDR,
        })
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

/// Excess precision is REFUSED, never silently rounded.
///
/// This is the failure that killed `rust_decimal` for this crate (E2): its `from_str`
/// silently rounded out-of-domain input and returned `Ok`. A money parser that rounds is a
/// money parser that loses money quietly.
#[test]
fn excess_precision_is_refused_not_rounded() {
    assert!(Money::<USD>::from_str("USD 0.0000000000000000005").is_err());
    assert!(Money::<IDR>::from_str("IDR 1.1234567890123456789").is_err());
}

proptest! {
    /// THE round-trip property, stated honestly as a retraction.
    ///
    /// `parse(render(v)) == v` for every value in the domain. The converse does NOT hold —
    /// `render(parse(s)) == s` fails for "USD 10.5", which parses fine but renders as
    /// "USD 10.50" — which is precisely why C7's "bijection" was downgraded.
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

    /// Rendering is CANONICAL: re-rendering a parsed canonical string changes nothing.
    #[test]
    fn prop_render_is_idempotent_through_a_parse(units in -DOMAIN_MAX..=DOMAIN_MAX) {
        let once = usd(units).to_string();
        let twice = Money::<USD>::from_str(&once).unwrap().to_string();
        prop_assert_eq!(once, twice);
    }
}

// ---------------------------------------------------------------------------------------
// Rate: ISO 15022 field 92B's shape, `BASE/QUOTE/RATE`.
//
// Format `:4!c//3!a/3!a/15d`, e.g. `:92B::EXCH//GBP/USD/1,619` meaning 1,00 GBP = 1,619 USD.
// First code is the BASE, second is the QUOTE. One deviation, documented: the decimal
// separator is a POINT, not SWIFT's comma, because C7's whole reason for using a string is
// exact decimal transport into JavaScript and `parseFloat("1,619")` is `1`.
// ---------------------------------------------------------------------------------------

use kamu_money_core::Rate;

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
    #[test]
    /// Positive-only since H1 (2026-07-27): a rate is a price. The round trip is the property
    /// under test, and it is unchanged -- only the set of values a `Rate` can hold moved.
    #[test]
    fn prop_rate_parse_of_render_is_the_identity(units in 1..=DOMAIN_MAX) {
        let r = Rate::<USD, IDR>::try_from_units(units).unwrap();
        prop_assert_eq!(Rate::<USD, IDR>::from_str(&r.to_string()).unwrap(), r);
    }
}

// ---------------------------------------------------------------------------------------
// The runtime-currency codec: `text::render` / `text::parse` / `text::parse_amount`.
//
// A PostgreSQL type cannot be generic, so `kamu-money-pg` cannot reach `Money<C>`'s `Display` or
// `FromStr` and would otherwise reimplement the trim rule (C9 forbids exactly that — an
// adapter is thin over ONE codec). These entry points exist so the database and the Rust
// program share an implementation rather than a specification. What follows tests the
// property that makes the sharing worth anything: the two paths cannot disagree.
// ---------------------------------------------------------------------------------------

use kamu_money_core::Iso4217;
use kamu_money_core::text;

/// Every currency, both paths, same string. `Display` delegates to `render`, so this pins the
/// delegation rather than a coincidence.
#[test]
fn the_runtime_codec_renders_exactly_what_display_renders() {
    let units = 10_500_000_000_000_000_000; // 10.5
    assert_eq!(text::render(units, Iso4217::USD).unwrap(), usd(units).to_string());
    assert_eq!(
        text::render(units, Iso4217::JPY).unwrap(),
        Money::<JPY>::try_from_units(units).unwrap().to_string()
    );
    assert_eq!(
        text::render(units, Iso4217::KWD).unwrap(),
        Money::<KWD>::try_from_units(units).unwrap().to_string()
    );
    assert_eq!(
        text::render(units, Iso4217::XAU).unwrap(),
        Money::<XAU>::try_from_units(units).unwrap().to_string()
    );
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

    /// **Whatever the renderer accepts, the parser must accept back.** Sampled across the
    /// FULL `i128` range, not just the domain — that is the point. `render` used to take any
    /// `i128` and emit canonical-looking text for values `parse` refuses, so the pair looked
    /// total and was not. Now the two agree on their input set by construction.
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
            Err(_) => prop_assert!(!kamu_money_core::advanced::domain::in_domain(units)),
        }
    }
}

/// The raw-unit entry points documented their domain precondition and did not enforce it.
/// `i128::MAX` went in and out-of-domain values came back — parts no `Money` constructor
/// would admit, returned as though they were money.
#[test]
fn the_raw_unit_entry_points_refuse_values_no_money_could_hold() {
    use core::num::NonZeroU32;
    use kamu_money_core::Rounding;
    use kamu_money_core::advanced::arithmetic::allocate_units;
    use kamu_money_core::advanced::arithmetic::div_int_units;

    let three = NonZeroU32::new(3).unwrap();
    for out_of_domain in [i128::MAX, i128::MIN, DOMAIN_MAX + 1, -DOMAIN_MAX - 1] {
        assert!(text::render(out_of_domain, Iso4217::USD).is_err(), "render accepted {out_of_domain}");
        assert!(allocate_units(out_of_domain, &[1, 1]).is_err(), "allocate_units accepted {out_of_domain}");
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
