//! Raw-unit and persistence surfaces for specialist integrations.
//!
//! Ordinary money code should use the crate-root types. These modules expose
//! the lower-level contracts needed by database extensions, storage codecs, and
//! invariant tests.

/// Checked arithmetic over untagged canonical units.
pub mod arithmetic {
    pub use crate::allocate_impl::allocate_units;
    pub use crate::arith_impl::{UnitSum, add_units, div_int_units, sub_units, sum_units};
}

/// Fixed-scale bounds and their checked predicate.
pub mod domain {
    pub use crate::domain_impl::{DOMAIN_MAX, POW10_SCALE, PRECISION, SCALE, in_domain};
}

/// Explicit residue values and the runtime-currency division form.
pub mod residue {
    pub use crate::residue_impl::{Division, Residue, UntaggedDivision};
}

/// Versioned hashes for values persisted outside this process.
pub mod stable_hash {
    pub use crate::stable_hash_impl::{STABLE_HASH_VERSION, fold_to_i32, stable_hash};
}
