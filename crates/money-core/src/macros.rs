//! The `money!` literal macro.
//!
//! # What this is, and what it is not
//!
//! This is **ergonomics, not a safety mechanism**, and saying so here is deliberate — the
//! feature looks like a shift-left and is not one.
//!
//! A hardcoded money literal is not a load-bearing runtime check. Amounts that matter arrive at
//! run time, from a database column, a bank file, an API payload; no macro touches those.
//! Literals live in test fixtures, seed data and config defaults, where a wrong value fails
//! loudly on the first test run at no production cost.
//!
//! What this buys is that the compile-time path, which **already existed**, stops being
//! unusable. This was always correct:
//!
//! ```
//! use kamu_money_core::{Money, iso::USD};
//! const RENT: Money<USD> = match Money::try_from_units(1_500_000_000_000_000_000_000) {
//!     Ok(amount) => amount,
//!     Err(_) => panic!("outside the money domain"),
//! };
//! ```
//!
//! and nobody was ever going to write it — note the raw canonical units, which no reviewer can
//! read as `1500.00`.

/// Build a [`Money`](crate::Money) constant from a decimal literal, checked at compile time.
///
/// Expands to a `const`, so an out-of-domain or over-scaled literal is a **build failure**, not
/// a runtime `unwrap`. The literal is read by [`text::parse_amount`](crate::text::parse_amount)
/// — the crate's only parser, which gained `const` for exactly this — so `money!` cannot accept
/// an amount that [`FromStr`](core::str::FromStr) would reject.
///
/// ```
/// use kamu_money_core::{money, Money, iso::USD};
///
/// const RENT: Money<USD> = money!(USD, "1500.00");
/// assert_eq!(RENT.units(), 1_500_000_000_000_000_000_000);
///
/// // Usable as an ordinary expression too.
/// assert_eq!(money!(USD, "-0.000000000000000001").units(), -1);
/// ```
///
/// The currency is a type, so the literal carries no currency code; a tagged `"USD 1500.00"`
/// string is the *runtime* form, parsed by `FromStr`.
#[macro_export]
macro_rules! money {
    ($currency:ty, $literal:literal) => {{
        // A named `const` rather than an inline `const` block: the item forces evaluation at
        // compile time in every position the macro can appear in, including one where an
        // expression would otherwise be free to be computed at run time.
        const AMOUNT: $crate::Money<$currency> = match $crate::text::parse_amount($literal) {
            Ok(units) => match $crate::Money::<$currency>::try_from_units(units) {
                Ok(amount) => amount,
                // Unreachable: `parse_amount` applies the domain. Present because the
                // constructor owns that rule, and a macro that assumed it would be the second
                // place the domain is decided.
                Err(_) => panic!("money! literal is outside the money domain"),
            },
            // The arms are split so the build failure names what is wrong with the literal.
            // `Display` is not const, so the message cannot carry the offending value.
            Err($crate::errors::ParseMoneyError::ExcessPrecision { .. }) => {
                panic!("money! literal carries more fractional digits than the canonical scale")
            }
            Err($crate::errors::ParseMoneyError::Amount(_)) => {
                panic!("money! literal is outside the money domain")
            }
            Err(_) => panic!("money! literal is not a valid decimal amount"),
        };
        AMOUNT
    }};
}

#[cfg(test)]
mod tests {
    use crate::Money;
    use crate::domain::DOMAIN_MAX;
    use crate::iso::USD;

    const RENT: Money<USD> = money!(USD, "1500.00");
    const REFUND: Money<USD> = money!(USD, "-1500.00");
    const SMALLEST: Money<USD> = money!(USD, "0.000000000000000001");
    const EDGE: Money<USD> = money!(USD, "999999999999999999.999999999999999999");

    #[test]
    fn the_macro_reads_a_literal_as_the_amount_a_reviewer_reads() {
        assert_eq!(RENT.units(), 1_500_000_000_000_000_000_000);
        assert_eq!(REFUND.units(), -1_500_000_000_000_000_000_000);
        assert_eq!(SMALLEST.units(), 1);
        assert_eq!(EDGE.units(), DOMAIN_MAX);
    }

    #[test]
    fn the_macro_and_the_runtime_parser_cannot_disagree() {
        // The whole reason `parse_fixed_point` gained `const` instead of gaining a const twin.
        // `FromStr` reads the *tagged* form, because it checks the currency code against `C` before
        // it reads any digits; `money!` takes the currency as a type, so its literal is bare. Both
        // reach the same parser underneath, which is what this compares.
        for (tagged, expected) in [
            ("USD 1500.00", RENT),
            ("USD -1500.00", REFUND),
            ("USD 0.000000000000000001", SMALLEST),
            ("USD 999999999999999999.999999999999999999", EDGE),
        ] {
            let parsed: Money<USD> = tagged.parse().expect("the macro accepted this amount");
            assert_eq!(parsed, expected, "FromStr and money! disagreed on {tagged:?}");
        }
    }

    #[test]
    fn the_macro_is_usable_as_an_ordinary_expression() {
        assert_eq!((money!(USD, "10.50") + money!(USD, "0.50")).units(), 11_000_000_000_000_000_000);
    }
}
