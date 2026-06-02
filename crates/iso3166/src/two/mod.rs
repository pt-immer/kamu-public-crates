//! ISO 3166-2 subdivisions.

mod generated {
    #![allow(clippy::all, clippy::pedantic, clippy::nursery)]
    include!(concat!(env!("OUT_DIR"), "/two_generated.rs"));

    // The generated module defines `pub const ALL_SUBDIVISIONS: &[Subdivision]`
    // which references the `Subdivision` type. That type is defined in the
    // parent module and re-imported here so `quote!`-generated code that
    // references `Subdivision` by simple name resolves.
    pub use super::subdivision::Subdivision;
}

mod subdivision;

pub use generated::{ALL_SUBDIVISIONS, Category, SUBDIVISION_COUNT};
pub use subdivision::Subdivision;

pub(crate) use generated::{SUBDIVISION_BY_CODE, subdivisions_of_generated};

#[cfg(feature = "serde")]
pub(crate) fn category_from_known_str(raw: &str) -> Option<Category> {
    generated::category_from_known_str_generated(raw)
}

use crate::one::Alpha2;

/// Return the slice of subdivisions whose parent is `country`, in
/// deterministic order. Empty if the country has no subdivisions.
#[must_use]
pub fn subdivisions_of(country: Alpha2) -> &'static [Subdivision] {
    subdivisions_of_generated(country)
}
