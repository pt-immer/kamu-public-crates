//! Error types for parsing ISO 3166 codes.

use thiserror::Error;

/// Error returned when parsing an ISO 3166 value fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseCountryError {
    /// The provided string was not a recognized ISO 3166-1 alpha-2 code.
    #[error("not a recognized ISO 3166-1 alpha-2 country code")]
    InvalidAlpha2,
    /// The provided string was not a recognized ISO 3166-1 alpha-3 code.
    #[error("not a recognized ISO 3166-1 alpha-3 country code")]
    InvalidAlpha3,
    /// The provided value was not a recognized ISO 3166-1 numeric code.
    #[error("not a recognized ISO 3166-1 numeric country code")]
    InvalidNumeric,
    /// The numeric value was outside the 3-digit ISO range `0..=999`.
    #[error("numeric value out of range (expected 0..=999)")]
    NumericOutOfRange,
    /// The input length did not match the expected fixed length.
    #[error("input length {got} does not match expected {expected}")]
    InvalidLength {
        /// Expected length.
        expected: u8,
        /// Actual length in bytes.
        got: usize,
    },
    /// The input contained non-ASCII bytes.
    #[error("input contains non-ASCII bytes")]
    NonAscii,
    /// The numeric digits could not be parsed as an integer.
    #[error("input is not a valid unsigned integer")]
    NotAnInteger,
}

/// Error returned when parsing an ISO 3166-2 subdivision code fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseSubdivisionError {
    /// The input did not contain a `-` separator at position 2.
    #[error("subdivision code is missing the '-' separator at position 2")]
    MissingSeparator,
    /// The country prefix was not a valid ISO 3166-1 alpha-2 code.
    #[error("subdivision code parent is not a valid ISO 3166-1 alpha-2 code")]
    InvalidParent,
    /// The full code did not match any known subdivision.
    #[error("not a recognized ISO 3166-2 subdivision code")]
    UnknownSubdivision,
    /// The input contained non-ASCII bytes.
    #[error("input contains non-ASCII bytes")]
    NonAscii,
    /// The input length was not in the expected range (4..=6).
    #[error("subdivision code length {got} is out of range (expected 4..=6)")]
    InvalidLength {
        /// Actual length in bytes.
        got: usize,
    },
}
