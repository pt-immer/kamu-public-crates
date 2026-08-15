//! Failures of serde decoding.

use super::CurrencyMismatch;
use super::{AmountError, ParseMoneyError, RateError};
use thiserror::Error;

/// Failure to decode a serde representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum WireError {
    /// Money text is invalid.
    #[error(transparent)]
    Parse(#[from] ParseMoneyError),
    /// Rate text is invalid.
    #[error(transparent)]
    Rate(#[from] RateError),
    /// Raw amount units are invalid.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// A wire tag disagrees with the target type.
    #[error(transparent)]
    WrongCurrency(#[from] CurrencyMismatch),
}
