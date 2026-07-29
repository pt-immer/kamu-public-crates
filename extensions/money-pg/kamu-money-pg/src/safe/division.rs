//! Division with an explicit residue.
//!
//! SQL cannot express `Division<C>` typestate, so `kmoney_div` returns quotient
//! and residue together.

use super::{kmoney, validated_or_error};
use kamu_money_core::Rounding;
use pgrx::prelude::*;

/// Divide an amount, returning the quotient **and the residue** as one row.
///
/// Rust's `Division<C>` withholds the quotient until
/// `take_residue()` or `discard_deliberately()`. PostgreSQL composite columns
/// can be omitted, so SQL cannot enforce that transition; it only makes the
/// residue visible beside the quotient:
///
/// ```sql
/// SELECT * FROM kmoney_div('USD 10.00', 3, 'toward_zero');
/// --         quotient         |         residue
/// --  USD 3.333333333333333333 | USD 0.000000000000000001
///
/// -- Explicitly project the quotient to omit the residue:
/// SELECT (kmoney_div('USD 10.00', 3, 'toward_zero')).quotient;
/// ```
///
/// The quotient stays at the canonical 18-digit scale; this function does not
/// round to the currency's minor unit.
/// `quotient * n + residue = amount` holds for every rounding mode. Use Rust
/// when residue handling must be enforced. [`kmoney_allocate`] conserves the
/// input without returning a residue.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_div(
    amount: kmoney,
    parts: i32,
    rounding: &str,
) -> TableIterator<'static, (name!(quotient, kmoney), name!(residue, kmoney))> {
    let amount = validated_or_error(amount.payload(), "kmoney_div");
    let code = amount.currency().numeric();

    let Ok(parts) = u32::try_from(parts) else {
        error!("kmoney_div: cannot divide into {parts} parts");
    };
    let Some(parts) = core::num::NonZeroU32::new(parts) else {
        error!("kmoney_div: cannot divide into zero parts");
    };

    // Require the caller to select the rounding policy.
    let Some(mode) = Rounding::from_name(rounding) else {
        error!("kmoney_div: {rounding:?} is not a rounding mode; expected one of: {}", Rounding::names());
    };

    let (quotient, residue) =
        kamu_money_core::advanced::arithmetic::div_int_units(amount.units(), parts, mode)
            .unwrap_or_else(|e| error!("kmoney_div: stored amount cannot be divided: {e}"))
            .take_residue();

    TableIterator::once((kmoney::new(quotient, code), kmoney::new(residue, code)))
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// The residue is returned beside the quotient.
    #[pg_test]
    fn division_returns_the_residue_beside_the_quotient() {
        let (quotient, residue) = Spi::get_two::<String, String>(
            "SELECT quotient::text, residue::text
               FROM kmoney_div('USD 10.00', 3, 'toward_zero')",
        )
        .expect("query ran");

        assert_eq!(quotient.expect("not null"), "USD 3.333333333333333333");
        assert_eq!(residue.expect("not null"), "USD 0.000000000000000001");
    }

    /// SQL preserves `quotient * n + residue == amount` for every mode.
    #[pg_test]
    fn the_division_identity_holds_for_every_rounding_mode() {
        for mode in [
            "half_even",
            "half_away_from_zero",
            "half_toward_zero",
            "toward_zero",
            "away_from_zero",
            "floor",
            "ceil",
        ] {
            let reconstructed = Spi::get_one::<bool>(&format!(
                "SELECT (
                     SELECT kmoney_sum(VARIADIC array_agg(q)) FROM (
                         SELECT quotient AS q FROM kmoney_div('USD 10.00', 3, '{mode}')
                         UNION ALL SELECT quotient FROM kmoney_div('USD 10.00', 3, '{mode}')
                         UNION ALL SELECT quotient FROM kmoney_div('USD 10.00', 3, '{mode}')
                         UNION ALL SELECT residue  FROM kmoney_div('USD 10.00', 3, '{mode}')
                     ) parts
                 )::text = 'USD 10.00'"
            ))
            .unwrap_or_else(|e| panic!("mode {mode}: {e}"))
            .expect("not null");
            assert!(reconstructed, "quotient*3 + residue != 10.00 for {mode}");
        }
    }

    /// SQL requires a recognized rounding mode.
    #[pg_test(
        error = "kmoney_div: \"bankers\" is not a rounding mode; expected one of: half_even, half_away_from_zero, half_toward_zero, toward_zero, away_from_zero, floor, ceil"
    )]
    fn division_refuses_an_unknown_rounding_mode() {
        Spi::get_one::<String>("SELECT quotient::text FROM kmoney_div('USD 10.00', 3, 'bankers')").ok();
    }
}
