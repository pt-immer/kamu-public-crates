//! Conservative distribution in SQL: `kmoney_allocate` and its output cap.
//!
//! The 65,536-part cap lives here with the function it bounds: it is the SQL adapter's
//! obligation, and `kamu-money-core` deliberately declines to invent a universal one.

use super::{kmoney, validated_or_error};
use pgrx::prelude::*;

/// The largest number of weights `kmoney_allocate` will accept in one call.
///
/// # Why there is a limit at all
///
/// The weights arrive as a SQL array, so their count is chosen at run time by whoever wrote the
/// query — and a PostgreSQL array is only bounded by the 1GB varlena limit, which is ~268 million
/// `int4`s. Allocating that many parts materialises four vectors at once (the incoming weights,
/// the checked `u32`s, the `i128` shares, and the `kmoney` results), roughly 56 bytes per part, so
/// an accepted call could ask a backend for something like 15GB. A Rust allocation failure is an
/// `abort`, not an unwindable error, which in a database backend takes the whole process with it.
///
/// So the boundary states a limit instead of discovering one.
///
/// # Why 2^16 and not something larger
///
/// The limit has to be **cheap to test at the boundary**, because an untested limit is the one
/// that rots. At 65,536 the acceptance case costs ~3.7MB and runs in the ordinary case suite on
/// every engine, so both sides of the comparison — one under, one over — are exercised on
/// PostgreSQL 15-18 *and* on Yugabyte every time. A limit of 2^20 would have been ~59MB and one
/// SQL array of a million elements per run, which is precisely the kind of cost that gets a test
/// quietly marked unportable and then stops covering anything.
///
/// 65,536 shares in a single SQL call is already far past any real distribution; a caller who
/// genuinely needs more is describing an application-side fold, not one query.
///
/// This is the SQL adapter's obligation. A Rust caller of `allocate_units` owns its own bound,
/// exactly as it owns the size of any other `Vec` it asks for.
const MAX_ALLOCATE_PARTS: usize = 1 << 16;

/// Split an amount across `weights`, conserving it **exactly**.
///
/// # Why this exists and `/` does not
///
/// Division is the operation that can round, and in Rust it is guarded by a typestate:
/// `div_int` returns a `Division<C>` that will not surrender its quotient until the caller has
/// chosen `take_residue()` or `discard_deliberately()`. A returned `Residue<C>` is `#[must_use]`
/// but does not panic on drop.
///
/// **SQL cannot express that.** There is no way to write a PostgreSQL function whose result is
/// unusable until a second value has been dealt with; any composite column may be omitted. This
/// crate therefore exposes allocation, which has no residue to lose:
/// `kmoney_sum(VARIADIC kmoney_allocate(x, w)) = x` for every input.
///
/// If you need a quotient and a remainder, do it in Rust, where the typestate is real. That is
/// the honest boundary — see the `kamu_money_core` docs on `Division`.
///
/// ```sql
/// SELECT unnest(kmoney_allocate('USD 10.00', ARRAY[1, 1, 1]));
/// --  USD 3.333333333333333334     <- the odd unit lands on the first share
/// --  USD 3.333333333333333333
/// --  USD 3.333333333333333333     <- sums to exactly USD 10.00, always
/// ```
///
/// The split is at the **canonical scale**, not at the currency's minor unit: these are not
/// three payable amounts, they are three exact thirds. Conservation is the guarantee on offer,
/// and it is the one that matters when the shares go back into a ledger.
/// `Array<'_, i32>` borrows the detoasted varlena, and its O(1) `len()` lets the function reject
/// an oversized request before walking or copying elements. A `Vec<Option<i32>>` argument would
/// allocate during pgrx conversion before the body could enforce the cap.
// `Array` by value, not by reference, for the same reason `kmoney_sum` takes `VariadicArray` by
// value: pgrx's `#[pg_extern]` ABI takes the owned argument type to build the SQL wrapper, so
// clippy::needless_pass_by_value cannot be honoured here. The value is a BORROW of the detoasted
// varlena regardless -- moving it copies a handful of pointers, not the array.
#[allow(clippy::needless_pass_by_value)]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_allocate(amount: kmoney, weights: Array<'_, i32>) -> Vec<kmoney> {
    let amount = validated_or_error(amount.payload(), "kmoney_allocate");
    let code = amount.currency().numeric();

    // LENGTH FIRST. Everything below this point is per-element, so everything below this point is
    // work an oversized input must not be able to buy.
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

    // Every rejection below is a case where kamu_money_core would panic. Catching them here buys a
    // message that names the offending value, which a panic converted to an ereport does not.
    //
    // `with_capacity(len)` is safe to ask for BECAUSE of the check above: len is now known to be
    // at most 65 536, so this is a bounded allocation rather than one the caller sizes.
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
    // `all(== 0)`, not `sum() == 0`: the intent is "every weight is zero", and saying it
    // directly drops the widening that existed only to make the sum safe.
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

    /// Allocation conserves exactly — that is the whole reason it, and not `/`, is the
    /// operation SQL gets without ceremony.
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

    /// The odd unit lands on the first share, and the shares are at the canonical scale — not
    /// rounded to the currency's minor unit, which would have moved money silently.
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

    /// A zero-weight recipient receives nothing, including truncation remainder. One canonical
    /// unit across weights `[0, 1, 1]` leaves a
    /// 1-unit remainder that must reach the first POSITIVE slot (index 1), never the zero at
    /// index 0. This delegates to `kamu-money-core`'s `allocate_units` kernel.
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

    /// Weighted, not merely equal — and still conserving.
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

    /// The weight count is chosen by whoever wrote the query, so the boundary states its limit.
    ///
    /// One past the limit, not a wild number: a test that passes 268 million weights would prove
    /// the same thing while spending a gigabyte to do it, and would not notice an off-by-one in
    /// the comparison.
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

    /// ...and accepts the limit itself, so the refusal above is a boundary rather than a ceiling
    /// nobody can reach. Conservation still holds at that size.
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
