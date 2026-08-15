//! Narrow errors, grouped by the operation that can return them.
//!
//! Prefer these in library signatures. [`MoneyError`] is the optional application-boundary
//! wrapper for a caller that genuinely wants one catch-all type.

mod allocation;
mod amount;
mod locale;
mod mismatch;
mod parse;
mod rate;
mod wire;

pub use allocation::AllocationError;
pub use amount::AmountError;
pub use locale::LocaleError;
pub use mismatch::CurrencyMismatch;
pub use parse::ParseMoneyError;
pub use rate::RateError;
pub use wire::WireError;

use thiserror::Error;

/// Convenience wrapper for applications that want one money-domain error type.
///
/// Library operations return the narrower errors above. This wrapper exists for
/// application boundaries that deliberately erase that detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum MoneyError {
    /// A currency did not match the one required.
    #[error(transparent)]
    Currency(#[from] CurrencyMismatch),
    /// Amount construction or arithmetic failed.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// Text parsing failed.
    #[error(transparent)]
    Parse(#[from] ParseMoneyError),
    /// Allocation failed.
    #[error(transparent)]
    Allocation(#[from] AllocationError),
    /// Rate construction, parsing, or conversion failed.
    #[error(transparent)]
    Rate(#[from] RateError),
    /// Locale policy configuration or rendering failed.
    #[error(transparent)]
    Locale(#[from] LocaleError),
    /// Wire decoding failed.
    #[error(transparent)]
    Wire(#[from] WireError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SCALE;

    #[test]
    fn top_level_error_preserves_the_narrow_source() {
        let narrow = ParseMoneyError::ExcessPrecision { digits: SCALE + 1 };
        let broad = MoneyError::from(narrow);
        assert!(matches!(
            broad,
            MoneyError::Parse(ParseMoneyError::ExcessPrecision { digits }) if digits == SCALE + 1
        ));
    }
}
