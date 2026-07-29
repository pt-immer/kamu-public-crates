//! Exact SQL allocation with a bounded output count.

use super::{kmoney, validated_or_error};
use pgrx::prelude::*;

/// The largest number of weights `kmoney_allocate` will accept in one call.
///
/// A PostgreSQL array can approach 1 GB. Allocation materializes the input,
/// checked weights, shares, and results—about 56 bytes per part—and Rust
/// allocation failure aborts the backend. The 65,536 cap bounds this path near
/// 3.7 MB and keeps both boundary cases practical on PostgreSQL 15–18 and
/// `YugabyteDB`. Rust callers of `allocate_units` own their own limits.
const MAX_ALLOCATE_PARTS: usize = 1 << 16;

/// Split an amount across `weights`, conserving it **exactly**.
///
/// SQL cannot enforce Rust's `Division<C>` residue transition. Allocation has
/// no residue to omit:
/// `kmoney_sum(VARIADIC kmoney_allocate(x, w)) = x`.
///
/// ```sql
/// SELECT unnest(kmoney_allocate('USD 10.00', ARRAY[1, 1, 1]));
/// --  USD 3.333333333333333334
/// --  USD 3.333333333333333333
/// --  USD 3.333333333333333333
/// ```
///
/// Shares use the canonical 18-digit scale, not the currency's minor unit.
/// `Array<'_, i32>` borrows the detoasted varlena and exposes its length before
/// per-element conversion, so oversized inputs are rejected before allocation.
// pgrx's ABI requires `Array` by value; the value still borrows the varlena.
#[allow(clippy::needless_pass_by_value)]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_allocate(amount: kmoney, weights: Array<'_, i32>) -> Vec<kmoney> {
    let amount = validated_or_error(amount.payload(), "kmoney_allocate");
    let code = amount.currency().numeric();

    // Reject size before any per-element work or allocation.
    let len = weights.len();
    if len == 0 {
        error!(
            "kmoney_allocate: weights must not be empty — there is no way to split an amount \
             into no parts without destroying it"
        );
    }
    if len > MAX_ALLOCATE_PARTS {
        error!(
            "kmoney_allocate: {len} weights exceeds the limit of {MAX_ALLOCATE_PARTS}; a \
             distribution that large belongs in the application, not in one SQL call"
        );
    }

    // Validate core preconditions here to return SQL errors with the bad value.
    // The size check bounds this allocation.
    let mut checked = Vec::with_capacity(len);
    for weight in weights.iter() {
        let Some(weight) = weight else {
            error!("kmoney_allocate: NULL weight — a share of nothing is not a share of zero");
        };
        let Ok(weight) = u32::try_from(weight) else {
            error!("kmoney_allocate: weight {weight} is negative; a negative share is not a distribution");
        };
        checked.push(weight);
    }
    if checked.iter().all(|&w| w == 0) {
        error!("kmoney_allocate: weights sum to zero — the amount would have nowhere to go");
    }

    kamu_money_core::advanced::arithmetic::allocate_units(amount.units(), &checked)
        .unwrap_or_else(|e| error!("kmoney_allocate: stored amount cannot be allocated: {e}"))
        .into_iter()
        .map(|units| kmoney::new(units, code))
        .collect()
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// Allocation conserves the input exactly.
    #[pg_test]
    fn allocation_conserves_the_total_exactly() {
        let total = Spi::get_one::<String>(
            "SELECT kmoney_sum(VARIADIC array_agg(part))::text
               FROM unnest(kmoney_allocate('USD 10.00', ARRAY[1, 1, 1])) AS part",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(total, "USD 10.00");
    }

    /// The odd unit lands on the first share at canonical scale.
    #[pg_test]
    fn allocation_puts_the_odd_unit_on_the_first_share() {
        let shares = Spi::get_one::<String>(
            "SELECT string_agg(part::text, ' | ')
               FROM unnest(kmoney_allocate('USD 10.00', ARRAY[1, 1, 1])) AS part",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(shares, "USD 3.333333333333333334 | USD 3.333333333333333333 | USD 3.333333333333333333");
    }

    /// Remainder skips zero-weight recipients.
    #[pg_test]
    fn allocation_never_pays_a_zero_weight_recipient() {
        let shares = Spi::get_one::<String>(
            "SELECT string_agg(part::text, ' | ')
               FROM unnest(kmoney_allocate('USD 0.000000000000000001', ARRAY[0, 1, 1])) AS part",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(
            shares, "USD 0.00 | USD 0.000000000000000001 | USD 0.00",
            "the zero-weight slot must receive nothing; the remainder goes to the first positive slot"
        );
    }

    /// Weighted allocation also conserves the total.
    #[pg_test]
    fn allocation_honours_weights_and_still_conserves() {
        let total = Spi::get_one::<String>(
            "SELECT kmoney_sum(VARIADIC array_agg(part))::text
               FROM unnest(kmoney_allocate('IDR 16000.01', ARRAY[7, 2, 1])) AS part",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(total, "IDR 16000.01");
    }

    #[pg_test(error = "kmoney_allocate: weights sum to zero — the amount would have nowhere to go")]
    fn allocation_refuses_weights_that_sum_to_zero() {
        Spi::get_one::<String>("SELECT kmoney_allocate('USD 10.00', ARRAY[0, 0])::text").ok();
    }

    #[pg_test(error = "kmoney_allocate: NULL weight — a share of nothing is not a share of zero")]
    fn allocation_refuses_a_null_weight() {
        Spi::get_one::<String>("SELECT kmoney_allocate('USD 10.00', ARRAY[1, NULL])::text").ok();
    }

    /// One past the limit is rejected.
    #[pg_test(
        error = "kmoney_allocate: 65537 weights exceeds the limit of 65536; a distribution that large belongs in the application, not in one SQL call"
    )]
    fn allocation_refuses_more_parts_than_the_documented_limit() {
        Spi::get_one::<String>(
            "SELECT kmoney_allocate('USD 10.00', \
             (SELECT array_agg(1) FROM generate_series(1, 65537)))::text",
        )
        .ok();
    }

    /// The limit itself is accepted and still conserves the total.
    #[pg_test]
    fn allocation_accepts_exactly_the_documented_limit() {
        let total = Spi::get_one::<String>(
            "SELECT kmoney_sum(VARIADIC kmoney_allocate('USD 10.00', \
             (SELECT array_agg(1) FROM generate_series(1, 65536))))::text",
        )
        .expect("query ran")
        .expect("row");
        assert_eq!(total, "USD 10.00", "allocation conserves at the limit too");
    }
}
