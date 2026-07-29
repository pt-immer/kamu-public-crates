//! Lazy, conserving distribution.
//!
//! [`Money::allocate`](crate::Money::allocate) handles weighted allocation.
//! [`Money::split`](crate::Money::split) returns [`SplitParts`] for an
//! allocation-free equal split; collect explicitly with
//! [`Money::split_collect`](crate::Money::split_collect).

pub use crate::allocate_impl::SplitParts;
