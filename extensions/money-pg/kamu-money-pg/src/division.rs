//! Division with an explicit residue: `kmoney_div`.
//!
//! Split out of `lib.rs` on 2026-07-27. The code is UNCHANGED -- this file is
//! a relocation, verified by `just schema-hash`, which fingerprints the generated SQL surface
//! with pgrx's non-reproducible ordering normalised away (E21).
//!
//! Returns quotient AND residue as a row, because SQL has no typestate to enforce what
//! `Division<C>` enforces in Rust -- so the residue is handed back rather than guarded.

use super::{currency_or_error, kmoney};
use kamu_money_core::Rounding;
use pgrx::prelude::*;

/// Divide an amount, returning the quotient **and the residue** as one row.
///
/// # The guarantee that does not survive SQL, stated plainly
///
/// In Rust this operation is guarded by a typestate: `div_int` returns a `Division<C>` that
/// will not surrender its quotient until the caller has named an exit — `take_residue()` or
/// `discard_deliberately()` — and a `Residue` dropped in silence detonates.
///
/// **SQL cannot express that.** There is no way to write a PostgreSQL function whose result is
/// unusable until a second value has been dealt with; any column of a composite can simply not
/// be selected, and no amount of API design changes that. So the residue is returned *beside*
/// the quotient, where ignoring it takes an explicit act that shows up in the query text:
///
/// ```sql
/// SELECT * FROM kmoney_div('USD 10.00', 3, 'toward_zero');
/// --         quotient         |         residue
/// --  USD 3.333333333333333333 | USD 0.000000000000000001
///
/// -- dropping the residue is possible, but you had to write .quotient to do it:
/// SELECT (kmoney_div('USD 10.00', 3, 'toward_zero')).quotient;
/// ```
///
/// Note the quotient keeps **all eighteen digits**. Nothing here rounds to a currency's minor
/// unit: `USD 10.00 / 3` is not `USD 3.33`, and a function that returned `3.33` would have
/// silently moved the other `0.003333…` somewhere. Presenting a payable figure is a separate
/// act, performed once, at the point of payment — §0.1's *display pads, never rounds*.
///
/// `quotient * n + residue = amount` holds exactly, for every rounding mode. If you need the
/// residue *enforced* rather than merely returned, do the division in Rust — that is the
/// honest boundary, and [`kmoney_allocate`] is the SQL operation that has no residue at all.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_div(
    amount: kmoney,
    parts: i32,
    rounding: &str,
) -> TableIterator<'static, (name!(quotient, kmoney), name!(residue, kmoney))> {
    let code = amount.code();
    let _ = currency_or_error(code, "kmoney_div");

    let Ok(parts) = u32::try_from(parts) else {
        error!("kmoney_div: cannot divide into {parts} parts");
    };
    let Some(parts) = core::num::NonZeroU32::new(parts) else {
        error!("kmoney_div: cannot divide into zero parts");
    };

    // No default mode, for the reason kamu_money_core gives: a default rounding mode is a decision
    // made by whoever wrote the library rather than whoever owns the money.
    let Some(mode) = Rounding::from_name(rounding) else {
        error!("kmoney_div: {rounding:?} is not a rounding mode; expected one of: {}", Rounding::names());
    };

    let (quotient, residue) = kamu_money_core::arith::div_int_units(amount.units(), parts, mode)
        .unwrap_or_else(|e| error!("kmoney_div: stored amount cannot be divided: {e}"))
        .take_residue();

    TableIterator::once((kmoney::new(quotient, code), kmoney::new(residue, code)))
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    // -----------------------------------------------------------------------------------
    // Division and allocation: the residue, and the operation that has none.
    // -----------------------------------------------------------------------------------

    /// The residue comes back **beside** the quotient. SQL cannot force the caller to look at
    /// it — that guarantee does not cross the boundary — but it cannot be produced without
    /// being returned either.
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

    /// `quotient * n + residue == amount`, exactly, for every mode. This is the identity the
    /// residue exists to preserve, checked in SQL rather than assumed from the Rust tests.
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

    /// No default rounding mode, in SQL either — a default is a decision made by whoever wrote
    /// the library rather than whoever owns the money.
    #[pg_test(
        error = "kmoney_div: \"bankers\" is not a rounding mode; expected one of: half_even, half_away_from_zero, half_toward_zero, toward_zero, away_from_zero, floor, ceil"
    )]
    fn division_refuses_an_unknown_rounding_mode() {
        Spi::get_one::<String>("SELECT quotient::text FROM kmoney_div('USD 10.00', 3, 'bankers')").ok();
    }
}
