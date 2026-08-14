//! Failures of weighted distribution.

use super::AmountError;
use thiserror::Error;

/// Failure to distribute an amount across weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum AllocationError {
    /// Raw units supplied to the untyped kernel lie outside the money domain.
    #[error(transparent)]
    Amount(#[from] AmountError),
    /// No positive claim exists.
    #[error("cannot allocate across {weights} weights because none is positive")]
    InvalidWeights {
        /// Number of supplied weights.
        weights: usize,
    },
}
