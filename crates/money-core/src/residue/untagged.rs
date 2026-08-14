//! The runtime-currency division form, for adapters that learn their currency at run time.

/// Runtime-currency division result for adapters.
///
/// Prefer [`Division`] in typed Rust code. This form exposes raw units because
/// adapters such as a PostgreSQL extension learn currency identity at runtime.
#[must_use = "resolve this division with .take_residue() or .discard_deliberately()"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UntaggedDivision {
    quotient: i128,
    residue: i128,
}

impl UntaggedDivision {
    #[inline]
    pub(crate) const fn new(quotient: i128, residue: i128) -> Self {
        Self { quotient, residue }
    }

    /// Return both unit counts and transfer accounting responsibility.
    #[must_use]
    pub const fn take_residue(self) -> (i128, i128) {
        (self.quotient, self.residue)
    }

    /// Return the quotient while explicitly accepting loss of the residue.
    #[must_use]
    pub const fn discard_deliberately(self) -> i128 {
        self.quotient
    }

    /// Inspect the residue without resolving the division.
    #[must_use]
    pub const fn residue_units(&self) -> i128 {
        self.residue
    }
}
