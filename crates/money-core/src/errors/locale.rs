//! Failures of locale policy configuration and rendering.

use super::AmountError;
use crate::domain::SCALE;
use crate::iso::Iso4217;
use thiserror::Error;

/// Failure to configure or apply a locale display policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum LocaleError {
    /// Raw units supplied to the runtime-currency renderer lie outside the money domain.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// The policy belongs to a different currency.
    #[error(
        "wrong currency: expected {}, found {}",
        expected.alpha3(),
        found.alpha3()
    )]
    WrongCurrency {
        /// Currency configured by the policy.
        expected: Iso4217,
        /// Currency of the value being rendered.
        found: Iso4217,
    },
    /// A minimum fraction width exceeds the fixed scale.
    #[error("{digits} fraction digits exceeds the supported scale of {SCALE}")]
    FractionDigitsOutOfRange {
        /// Rejected width.
        digits: u8,
    },
    /// A zero grouping width would make grouping fail to progress.
    #[error("grouping width at index {index} must be positive")]
    ZeroGroupingWidth {
        /// Index of the rejected entry.
        index: usize,
    },
    /// An empty decimal separator would make output ambiguous.
    #[error("decimal separator must not be empty")]
    EmptyDecimalSeparator,
    /// Equal non-empty group and decimal separators make output ambiguous.
    #[error("group and decimal separators must differ")]
    AmbiguousSeparators,
}
