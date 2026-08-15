//! Failures of rate construction, parsing and conversion.

use super::{AmountError, ParseMoneyError};
use crate::iso::Iso4217;
use thiserror::Error;

/// Failure to construct, parse, or apply an exchange rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RateError {
    /// Rate units lie outside the money domain.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// Rates are prices and must be strictly positive.
    #[error("rate must be strictly positive; got {attempted_units} canonical units")]
    NonPositive {
        /// Rejected rate units.
        attempted_units: i128,
    },
    /// A conversion result lies outside the money domain.
    #[error(
        "{} to {} conversion exceeds the money domain",
        from.alpha3(),
        to.alpha3()
    )]
    ConversionOverflow {
        /// Currency converted from.
        from: Iso4217,
        /// Currency converted to.
        to: Iso4217,
    },
    /// Textual rate input is invalid.
    #[error(transparent)]
    Parse(#[from] ParseMoneyError),
}
