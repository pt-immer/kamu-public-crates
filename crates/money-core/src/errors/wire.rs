//! Failures of serde decoding.

use super::{AmountError, ParseMoneyError, RateError};
use crate::iso::Iso4217;
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
    #[error(
        "wrong currency: expected {}, found {}",
        expected.alpha3(),
        found.alpha3()
    )]
    WrongCurrency {
        /// Currency required by the target type.
        expected: Iso4217,
        /// Currency carried on the wire.
        found: Iso4217,
    },
}
