//! Failures of money and rate text parsing.

use super::AmountError;
use crate::domain::SCALE;
use crate::iso::Iso4217;
use thiserror::Error;

/// Failure to parse a monetary amount or tagged money literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseMoneyError {
    /// The input does not match the accepted grammar.
    #[error("invalid money literal")]
    InvalidSyntax,
    /// The input carries more fractional digits than the fixed scale.
    #[error("{digits} fractional digits exceeds the supported scale of {SCALE}")]
    ExcessPrecision {
        /// Fractional digits present in the input.
        digits: u32,
    },
    /// A positive magnitude cannot be represented as signed canonical units.
    #[error("positive money magnitude exceeds the parser range")]
    PositiveMagnitudeOverflow,
    /// A negative magnitude exceeds the representable magnitude of `i128::MIN`.
    #[error("negative money magnitude exceeds the parser range")]
    NegativeMagnitudeOverflow,
    /// A tagged literal names a different currency from the target type.
    #[error(
        "wrong currency: expected {}, found {}",
        expected.alpha3(),
        found.alpha3()
    )]
    WrongCurrency {
        /// Currency required by the target type.
        expected: Iso4217,
        /// Currency named by the input.
        found: Iso4217,
    },
    /// The parsed value fits `i128` but lies outside the money domain.
    #[error(transparent)]
    Amount(#[from] AmountError),
}
