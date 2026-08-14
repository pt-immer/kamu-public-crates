//! Failures of amount construction and arithmetic.

use crate::domain::DOMAIN_MAX;
use crate::domain::SCALE;
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
}
