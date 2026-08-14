//! The pre-facade compatibility paths were removed in 0.2.0 and must stay removed.
//!
//! Deleting a `#[doc(hidden)]` item is invisible: nothing in the crate references these, no test
//! exercises them, and re-adding one to quiet a downstream build would pass every other check.
//! Compiled as a downstream crate, this is what makes the removal a contract.
//!
//! One case rather than several, because the committed `.stderr` then also pins that each name
//! fails for its own reason — a module that no longer exists resolves differently from a root
//! re-export that no longer exists.
//!
//! `allocation`, `currency`, `domain` and `money` fail as `E0603` private rather than `E0432`
//! absent: the crate's own modules carry those names. The refusal is the same, and making one of
//! them `pub` would compile this file and fail the suite.

use kamu_money_core::allocation::SplitParts;
use kamu_money_core::currency::StaticCurrency;
use kamu_money_core::domain::SCALE;
use kamu_money_core::error::AmountError;
use kamu_money_core::money::Money;

fn main() {
    // Root re-exports, removed alongside the module aliases above.
    let _: Option<kamu_money_core::UntaggedDivision> = None;
    let _ = kamu_money_core::DOMAIN_MAX;
    let _ = kamu_money_core::ParseMoneyError::InvalidSyntax;
    let _: Option<(SplitParts, StaticCurrency, SCALE, AmountError, Money)> = None;
}
