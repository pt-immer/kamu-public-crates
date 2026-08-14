//! Locale-aware display: symbol, grouping, and a minimum fraction width.
//!
//! # Why this is not `Display`
//!
//! [`Display`](core::fmt::Display) is the canonical wire and database form. Locale-aware
//! rendering is a separate entry point so decoration cannot change stored data.
//!
//! Both forms share the fixed-point digit extraction. They may differ in separators and
//! decoration, but not in the represented number.
//!
//! # The one rule that constrains everything here
//!
//! Display pads but never rounds. A policy owns the minimum fraction width; the value's
//! significant digits determine the maximum.
//!
//! IDR settles at two decimal places but commonly displays with no required fraction. A value
//! such as `16000.50` still renders its significant fractional digit.
//!
//! ```
//! use kamu_money_core::iso::IDR;
//! use kamu_money_core::locale::ID_IDR;
//! use kamu_money_core::advanced::domain::POW10_SCALE;
//! use kamu_money_core::Money;
//!
//! let m = Money::<IDR>::try_from_units(16_000 * POW10_SCALE + POW10_SCALE / 2).unwrap();
//! assert_eq!(m.to_string(), "IDR 16000.50");            // canonical: settles at 2
//! assert_eq!(ID_IDR.render(m).unwrap(), "Rp 16.000,5"); // display: 0 minimum, nothing lost
//! ```
//!
//! # Scope
//!
//! This module is not a locale database. Its constants are examples; applications can build
//! policies from their own CLDR or ICU data. Negative values always use a leading `-`.

mod group;
mod policy;
mod render;

pub use policy::{DE_EUR, EN_USD, FractionDigits, ID_IDR, JA_JPY, LocalePolicy, SymbolPosition};
