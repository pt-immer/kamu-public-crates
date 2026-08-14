//! Integer division over raw canonical units.

use crate::errors::AmountError;
use crate::residue::UntaggedDivision;
use crate::rounding::{Rounding, div_round_i256};
use core::num::NonZeroU32;
use ethnum::I256;

/// The non-generic core of [`Money::div_int`], in canonical units.
///
/// For adapters that learn the currency at run time; see [`UntaggedDivision`] for why this is
/// not the API a Rust program should reach for.
///
/// # Errors
/// [`AmountError`] if `units` is outside the domain.
///
/// This arm replaces a doc comment that stated the precondition and left it unenforced —
/// "never, **for any in-domain** `units`" was a caller obligation nothing checked, and the
/// function accepted `i128::MAX` and returned a quotient outside the domain.
///
/// # Panics
/// Panics only if the rounding kernel returns a quotient larger than the dividend or a
/// residue at least as large as the divisor.
pub fn div_int_units(units: i128, n: NonZeroU32, mode: Rounding) -> Result<UntaggedDivision, AmountError> {
    if !crate::domain::in_domain(units) {
        return Err(AmountError::out_of_domain(units));
    }
    let (q, r) = div_round_i256(I256::from(units), I256::from(i128::from(n.get())), mode);
    // |q| <= |units| <= DOMAIN_MAX and |r| < n, so both conversions are total.
    let q = i128::try_from(q).expect("quotient magnitude cannot exceed the dividend");
    let r = i128::try_from(r).expect("residue magnitude is below the divisor");
    Ok(UntaggedDivision::new(q, r))
}
