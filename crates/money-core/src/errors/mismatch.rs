//! One currency mismatch, wherever it is found.

use crate::iso::Iso4217;
use thiserror::Error;

/// A currency that is not the one required.
///
/// Parsing, locale rendering and wire decoding each discover this at their own boundary and
/// each keep their own variant for it, but the pair and the message are defined once. Nothing
/// else can hold them in step: three copies of a format string agree until one of them is
/// edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("wrong currency: expected {}, found {}", expected.alpha3(), found.alpha3())]
pub struct CurrencyMismatch {
    /// The currency that was required.
    pub expected: Iso4217,
    /// The currency that arrived.
    pub found: Iso4217,
}
