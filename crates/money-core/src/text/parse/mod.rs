//! Reading money out of text. Liberal in what it accepts, and never rounding.

pub(crate) mod fixed_point;
mod tagged;

#[cfg(test)]
mod equivalence;

pub(crate) use fixed_point::parse_fixed_point;
#[cfg(feature = "serde")]
pub(crate) use fixed_point::parse_rate_amount;
pub(crate) use tagged::split_tagged;
pub use tagged::{parse, parse_amount};
