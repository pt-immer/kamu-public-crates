//! Errors grouped by operation.

use crate::domain::{DOMAIN_MAX, SCALE};
use crate::iso::Iso4217;
use ethnum::I256;
use thiserror::Error;

/// Failure to construct or compute a fixed-domain amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum AmountError {
    /// A canonical-unit value fits `i128` but lies outside the money domain.
    #[error(
        "{attempted_units} canonical units is outside the supported range \
         -{DOMAIN_MAX}..={DOMAIN_MAX}"
    )]
    OutOfDomain {
        /// The rejected value, in canonical `10^-18` units.
        attempted_units: i128,
    },
    /// Scaling whole currency units cannot be represented as canonical units.
    #[error("{attempted_major} whole currency units cannot be represented at scale {SCALE}")]
    MajorScaleOverflow {
        /// Whole-unit input supplied to `Money::try_from_major`.
        attempted_major: i128,
    },
    /// An exact total is known, and it is too wide to be a canonical `i128` unit count.
    ///
    /// Distinct from [`Self::ArithmeticOverflow`], which reports that no exact total could be
    /// computed at all. Here one was: a sum of in-domain terms needs only 171 of them at the
    /// domain edge to exceed `i128`, and the accumulator is holding the answer at the moment it
    /// is refused. Refusing is right; forgetting the value is not, so it travels with the
    /// refusal.
    #[error(
        "{} canonical units is outside the supported range -{DOMAIN_MAX}..={DOMAIN_MAX}",
        I256::from_le_bytes(*attempted_units)
    )]
    WideOutOfDomain {
        /// The exact total, as 32 little-endian bytes.
        ///
        /// Bytes rather than a wide integer, so the arithmetic backend stays out of this
        /// crate's public API. It is the encoding `advanced::UnitSum::to_le_bytes` produces,
        /// and `advanced::UnitSum::from_le_bytes` reads it back.
        attempted_units: [u8; 32],
    },
    /// A wide arithmetic result cannot be represented exactly for domain validation.
    ///
    /// Carries no value, because there is none: the accumulator itself overflowed, so no exact
    /// total was ever computed. When one exists, [`Self::WideOutOfDomain`] carries it.
    #[error("amount computation exceeds the supported arithmetic range")]
    ArithmeticOverflow,
}

impl AmountError {
    /// Build an out-of-domain error for canonical units.
    #[must_use]
    pub const fn out_of_domain(attempted_units: i128) -> Self {
        Self::OutOfDomain { attempted_units }
    }

    /// Build an out-of-domain error for an exact total too wide to narrow to `i128`.
    #[must_use]
    pub const fn wide_out_of_domain(attempted_units: [u8; 32]) -> Self {
        Self::WideOutOfDomain { attempted_units }
    }
}

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

/// Convenience wrapper for applications that want one money-domain error type.
///
/// Library operations return the narrower errors above. This wrapper exists for
/// application boundaries that deliberately erase that detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum MoneyError {
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

    #[test]
    fn amount_errors_report_only_values_that_were_observed() {
        let out_of_domain = AmountError::out_of_domain(DOMAIN_MAX + 1).to_string();
        assert!(out_of_domain.contains(&(DOMAIN_MAX + 1).to_string()));
        assert!(out_of_domain.contains(&DOMAIN_MAX.to_string()));

        let scale_overflow = AmountError::MajorScaleOverflow { attempted_major: i128::MIN }.to_string();
        assert!(scale_overflow.contains(&i128::MIN.to_string()));

        // Nothing exact was computed here, so nothing is named. The two probes are the values a
        // careless implementation would reach for as a stand-in.
        let arithmetic_overflow = AmountError::ArithmeticOverflow.to_string();
        assert!(!arithmetic_overflow.contains(&i128::MAX.to_string()));
        assert!(!arithmetic_overflow.contains(&i128::MIN.to_string()));

        // A wide total WAS observed, so it is named in full rather than approximated by the
        // `i128` edge it just crossed.
        let wide = I256::from(DOMAIN_MAX) * I256::from(171i128);
        let wide_out_of_domain = AmountError::wide_out_of_domain(wide.to_le_bytes()).to_string();
        assert!(wide_out_of_domain.contains(&wide.to_string()));
        assert!(
            !wide_out_of_domain.contains(&i128::MAX.to_string()),
            "the boundary that was crossed is not the value that crossed it"
        );
    }

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
