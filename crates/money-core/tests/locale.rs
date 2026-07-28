//! `LocalePolicy`: the display form, and the proof it cannot become a second number.
//! (DESIGN.md C2, §0.1)
//!
//! # What this file is really testing
//!
//! `Display` is **frozen**. It backs five consumers — itself, the serde wire (C7),
//! `kmoney`'s in/out (C8), the `postgres`/`sqlx` stored form (C9), and the phase-4/phase-5
//! differential that asserts they all agree. So a locale-aware renderer cannot be a change
//! to `Display`; it has to be a separate entry point that leaves every existing text
//! assertion in this repo standing untouched.
//!
//! That makes two properties worth more than the formatting itself:
//!
//! 1. **The canonical form does not move.** A policy existing, or being applied, changes
//!    nothing about what `to_string()` produces.
//! 2. **A policy pads, never rounds** (§0.1). IDR is the case the spec names: it *settles*
//!    at 2dp and *displays* at 0dp, so a naive "display exponent" implementation would drop
//!    real digits off `16000.50` and print a number that is not the stored one. §0.1 calls
//!    that a second number claiming to be the money, and rejects it.

use kamu_money_core::iso::{EUR, IDR, JPY, USD, XAU};
use kamu_money_core::locale::{DE_EUR, EN_USD, FractionDigits, ID_IDR, JA_JPY, LocalePolicy, SymbolPosition};
use kamu_money_core::{DOMAIN_MAX, Iso4217, LocaleError, Money, POW10_SCALE, text};
use proptest::prelude::*;

/// 16 000.50 IDR — the amount the spec's own C2 example turns on.
fn idr_16000_50() -> Money<IDR> {
    Money::<IDR>::try_from_units(16_000 * POW10_SCALE + POW10_SCALE / 2).expect("in domain")
}

/// **This test did not compile before 2026-07-27, and that is the whole assertion.**
///
/// The module docs instruct an application to build a policy from whatever CLDR/ICU source it
/// already carries. That instruction was unfollowable while every field was `&'static`: locale
/// data read from a file or a row at startup could reach a `LocalePolicy` only by being leaked,
/// cached in an unrelated global, or pasted back into source. The `'static` bought `const`
/// constructors — a compile-time convenience that had grown past the invariant it protected and
/// broken the documented runtime path.
///
/// A lifetime relaxation cannot be tested by asserting a value, because the old code produced
/// the same string for the same input; it can only be tested by *compiling* against data the
/// borrow checker knows is not `'static`. Every string below is heap-allocated at run time, so
/// `&symbol` is a `&'local str` and nothing here can be promoted.
///
/// Mutation-check: put `&'static` back on `LocalePolicy`'s fields and this file stops building.
#[test]
fn a_policy_can_be_built_from_data_loaded_at_run_time() {
    // Stand-ins for a CLDR/ICU table read at startup. `String`, not `&'static str`.
    let symbol = String::from("Rp ");
    let group = String::from(".");
    let decimal = String::from(",");
    let grouping: Vec<u8> = vec![3];

    let policy = LocalePolicy::new(Iso4217::IDR, &symbol)
        .try_with_separators(&group, &decimal)
        .unwrap()
        .try_with_grouping(&grouping)
        .unwrap()
        .with_min_fraction_digits(FractionDigits::ZERO);

    assert_eq!(policy.render(idr_16000_50()).unwrap(), "Rp 16.000,5");

    // And it is the same policy the crate ships as a constant, reached the other way round:
    // borrowed data and `'static` data are one type at two lifetimes, not two behaviours.
    assert_eq!(policy.render(idr_16000_50()).unwrap(), ID_IDR.render(idr_16000_50()).unwrap());
}

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

/// C2's motivating case, stated as the spec states it: IDR renders 0dp, settles 2dp.
#[test]
fn rupiah_displays_at_zero_dp_but_settles_at_two() {
    assert_eq!(Iso4217::IDR.exponent(), Some(2), "ISO settlement exponent is 2");
    let whole = Money::<IDR>::try_from_major(16_000).unwrap();
    assert_eq!(whole.to_string(), "IDR 16000.00", "settles at 2");
    assert_eq!(ID_IDR.render(whole).unwrap(), "Rp 16.000", "displays at 0");
}

/// **§0.1, mechanised.** A display minimum BELOW the value's own significant digits must
/// pad nothing and drop nothing. `16000.50` at a 0dp policy is `16.000,5`, never `16.000`.
///
/// Mutation-check, **measured** rather than asserted: adding
/// `fraction.truncate(min_fraction_digits)` to the renderer fails 7 of this file's 13 tests,
/// both property tests among them. An earlier version of this comment predicted that only
/// this test would catch it — which was a guess, and wrong. Worth recording as written,
/// because the blast radius is the actual result: "display pads, never rounds" is not one
/// assertion here, it is load-bearing under most of the file.
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

/// The last grouping size repeats. `[3]` is the western group-of-three; `[3, 2]` is the
/// Indian lakh/crore shape, which is the reason this is a slice and not a single number.
#[test]
fn the_last_grouping_size_repeats() {
    let big = Money::<USD>::try_from_major(12_345_678).unwrap();
    assert_eq!(EN_USD.render(big).unwrap(), "$12,345,678.00");

    // Indian digit grouping: 3, then 2 forever. Built by hand rather than shipped as a
    // named locale, because the table has no INR and inventing one to decorate a test
    // would be a fact this crate did not measure.
    let indian = LocalePolicy::new(Iso4217::USD, "$").try_with_grouping(&[3, 2]).unwrap();
    assert_eq!(indian.render(big).unwrap(), "$1,23,45,678.00");

    let ungrouped = LocalePolicy::new(Iso4217::USD, "$").try_with_grouping(&[]).unwrap();
    assert_eq!(ungrouped.render(big).unwrap(), "$12345678.00");
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
/// minimum is the ISO exponent, so JPY simply arrives at 0.
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
        Err(LocaleError::WrongCurrency { expected: Iso4217::IDR, found: Iso4217::USD })
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

#[test]
fn a_minimum_past_the_scale_is_rejected() {
    assert_eq!(FractionDigits::try_new(19), Err(LocaleError::FractionDigitsOutOfRange { digits: 19 }));
    let maximum = FractionDigits::try_new(18).unwrap();
    let policy = LocalePolicy::new(Iso4217::USD, "$").with_min_fraction_digits(maximum);
    assert_eq!(maximum.get(), 18);
    assert_eq!(policy.min_fraction_digits(), maximum);
}

#[test]
fn degenerate_locale_policies_are_rejected() {
    assert_eq!(
        LocalePolicy::new(Iso4217::USD, "$").try_with_grouping(&[3, 0]),
        Err(LocaleError::ZeroGroupingWidth { index: 1 })
    );
    assert_eq!(
        LocalePolicy::new(Iso4217::USD, "$").try_with_separators(".", "."),
        Err(LocaleError::AmbiguousSeparators)
    );
    assert_eq!(
        LocalePolicy::new(Iso4217::USD, "$").try_with_separators(",", ""),
        Err(LocaleError::EmptyDecimalSeparator)
    );

    let ungrouped = LocalePolicy::new(Iso4217::USD, "$").try_with_separators("", ".").unwrap();
    assert_eq!(ungrouped.render(Money::<USD>::try_from_major(1_000).unwrap()).unwrap(), "$1000.00");
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
