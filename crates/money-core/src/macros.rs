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
/// use kamu_money_core::{Money, iso::USD};
///
/// const RENT: Money<USD> = kamu_money_core::money!(USD, "1500.00");
/// assert_eq!(RENT.units(), 1_500_000_000_000_000_000_000);
///
/// // Usable as an ordinary expression too.
/// assert_eq!(kamu_money_core::money!(USD, "-0.000000000000000001").units(), -1);
/// ```
///
/// The currency is a type, so the literal carries no currency code; a tagged `"USD 1500.00"`
/// string is the *runtime* form, parsed by `FromStr`.
///
/// # Reach it by path until 0.2.0
///
/// The examples qualify the macro rather than importing it, because
/// `money` is *also* the deprecated compatibility module kept until 0.2.0 (hidden from these
/// docs, which is why it is not linked here).
/// A `use kamu_money_core::money;` imports both names and inherits that deprecation warning;
/// the macro still works, and the warning is about the module. Removing the module resolves
/// this, and removing it is itself the breaking change 0.2.0 already owns.
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
            Err($crate::ParseMoneyError::ExcessPrecision { .. }) => {
                panic!("money! literal carries more fractional digits than the canonical scale")
            }
            Err($crate::ParseMoneyError::Amount(_)) => {
                panic!("money! literal is outside the money domain")
            }
            Err(_) => panic!("money! literal is not a valid decimal amount"),
        };
        AMOUNT
    }};
}
