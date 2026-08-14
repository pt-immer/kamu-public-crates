//! Addition and subtraction over raw canonical units.

/// Add two canonical unit counts. `None` if **either operand** or the result is outside the
/// domain.
///
/// This is the shared `Money`/`kamu-money-pg` addition kernel: [`Money::try_add`](crate::Money::try_add) — and through
/// it [`Money::checked_add`](crate::Money::checked_add) and `+` — plus `kamu-money-pg`'s generated `<type>_add` functions
/// all delegate here.
#[inline]
#[must_use]
pub const fn add_units(a: i128, b: i128) -> Option<i128> {
    // Raw callers do not carry Money's invariant. Check operands as well as the result so
    // out-of-domain values cannot cancel into a valid-looking amount.
    if !crate::domain::in_domain(a) || !crate::domain::in_domain(b) {
        return None;
    }
    match a.checked_add(b) {
        Some(v) => {
            if crate::domain::in_domain(v) {
                Some(v)
            } else {
                None
            }
        }
        None => None,
    }
}

/// Subtract two canonical unit counts. `None` if **either operand** or the result is outside
/// the domain. The units-level kernel behind [`Money::try_sub`](crate::Money::try_sub) — and through it
/// [`Money::checked_sub`](crate::Money::checked_sub) and `-` — and `kamu-money-pg`'s generated `<type>_sub` functions.
#[inline]
#[must_use]
pub const fn sub_units(a: i128, b: i128) -> Option<i128> {
    // Operands enforced, not assumed — see `add_units`. Without this,
    // `sub_units(i128::MAX, i128::MAX)` returns `Some(0)`.
    if !crate::domain::in_domain(a) || !crate::domain::in_domain(b) {
        return None;
    }
    match a.checked_sub(b) {
        Some(v) => {
            if crate::domain::in_domain(v) {
                Some(v)
            } else {
                None
            }
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::arithmetic::{add_units, sub_units, sum_units};
    use crate::domain::DOMAIN_MAX;
    use crate::errors::AmountError;

    #[test]
    fn add_units_and_sub_units_are_the_shared_kernel() {
        assert_eq!(add_units(100, 200), Some(300));
        assert_eq!(sub_units(300, 200), Some(100));
        assert_eq!(add_units(DOMAIN_MAX, 0), Some(DOMAIN_MAX));
        assert_eq!(add_units(DOMAIN_MAX, 1), None, "one past the top leaves the domain");
        assert_eq!(sub_units(-DOMAIN_MAX, 1), None);
        assert_eq!(add_units(DOMAIN_MAX, -DOMAIN_MAX), Some(0));
    }

    /// These are PUBLIC functions over raw `i128`, so the domain is a precondition they must
    /// ENFORCE rather than assume — the typed wrapper's invariant does not reach them. The
    /// dangerous shape is cancellation: two operands that were never money summing to a
    /// perfectly ordinary answer. Checking only the result returns `Some(0)` here and hands a
    /// caller a value laundered out of corrupt input.
    #[test]
    fn the_raw_kernels_refuse_out_of_domain_operands_even_when_they_cancel() {
        // The cancellation cases: the RESULT is impeccable, the operands are not.
        assert_eq!(add_units(i128::MAX, -i128::MAX), None, "cancels to 0");
        assert_eq!(sub_units(i128::MAX, i128::MAX), None, "cancels to 0");
        assert_eq!(
            sum_units([i128::MAX, -i128::MAX]),
            Err(AmountError::out_of_domain(i128::MAX)),
            "the offending TERM names itself, not the total"
        );

        // The sharpest case for the OPERAND guard specifically: a bad operand whose RESULT is
        // impeccably in-domain. Without the guard this returns Some(DOMAIN_MAX) — a valid-looking
        // amount computed from an input that was never money. The `past, 0` style cases below
        // leave the domain in the result too, so they would fail with or without the guard.
        assert_eq!(add_units(DOMAIN_MAX + 1, -1), None, "result would be in-domain");
        assert_eq!(sub_units(DOMAIN_MAX + 1, 1), None, "result would be in-domain");

        // One past the top, on either side, in either operand position.
        let past = DOMAIN_MAX + 1;
        assert_eq!(add_units(past, 0), None);
        assert_eq!(add_units(0, past), None);
        assert_eq!(sub_units(-past, 0), None);
        assert_eq!(sub_units(0, -past), None);
        assert!(sum_units([1, past, 1]).is_err());

        // The domain edges themselves still pass: this refuses invalid input, not valid money.
        assert_eq!(add_units(DOMAIN_MAX, -DOMAIN_MAX), Some(0));
        assert_eq!(sum_units([DOMAIN_MAX, -DOMAIN_MAX]), Ok(0));
    }
}
