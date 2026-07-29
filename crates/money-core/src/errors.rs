//! Narrow errors grouped by the operation that can return them.
//!
//! Prefer these types in library signatures. [`MoneyError`] is the optional
//! application-boundary wrapper when one catch-all type is genuinely useful.

pub use crate::error_impl::{
    AllocationError, AmountError, LocaleError, MoneyError, ParseMoneyError, RateError, WireError,
};
