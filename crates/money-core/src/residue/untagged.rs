//! The runtime-currency division form, for adapters that learn their currency at run time.

/// Runtime-currency division result for adapters.
///
/// Prefer [`Division`](crate::Division) in typed Rust code. This form exposes raw units because
/// adapters such as a PostgreSQL extension learn currency identity at run time.
///
/// The residue enforcement is not weaker for it. Both exits consume `self`, and the type is
/// neither `Copy` nor `Clone`, so the quotient cannot be released a second time once the residue
/// has been decided. The single thing it does not carry is the currency.
#[must_use = "resolve this division with .take_residue() or .discard_deliberately()"]
#[derive(Debug, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use crate::Rounding;
    use crate::arithmetic::div_int_units;
    use core::num::NonZeroU32;

    #[test]
    fn untagged_division_exposes_the_same_two_decisions_for_adapters() {
        let three = NonZeroU32::new(3).unwrap();

        let division = div_int_units(10, three, Rounding::TowardZero).unwrap();
        assert_eq!(division.residue_units(), 1);
        assert_eq!(format!("{division:?}"), "UntaggedDivision { quotient: 3, residue: 1 }");
        assert_eq!(division.take_residue(), (3, 1));

        let quotient = div_int_units(10, three, Rounding::TowardZero).unwrap().discard_deliberately();
        assert_eq!(quotient, 3);
    }
}
