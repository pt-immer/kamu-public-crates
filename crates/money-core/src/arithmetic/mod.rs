//! Exact arithmetic. Rounding is not merely discouraged here — it is unrepresentable.
//!
//! The raw kernel in [`kernel`] owns every domain check; [`typed`] is the `Money<C>` skin over
//! it, and [`ops`] the operator surface. `advanced::arithmetic` republishes the kernel.

pub mod kernel;
mod ops;
mod typed;

pub use kernel::{UnitSum, add_units, allocate_units, div_int_units, sub_units, sum_units};
