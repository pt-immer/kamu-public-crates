//! Raw canonical units, with no currency attached.
//!
//! Every operand and every result is domain-checked here, because a caller reaching these
//! carries none of `Money<C>`'s invariant. This is the surface `advanced::arithmetic` publishes
//! and `kamu-money-pg` builds its SQL operators on.

mod add_sub;
mod allocate;
mod divide;
mod sum;

pub use add_sub::{add_units, sub_units};
pub use allocate::allocate_units;
pub use divide::div_int_units;
pub use sum::{UnitSum, sum_units};
