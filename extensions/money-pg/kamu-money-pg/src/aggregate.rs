//! `sum(kmoney)`: the row aggregate and its wide transition state.
//!
//! Split out of `lib.rs` on 2026-07-27. The code is UNCHANGED -- this file is
//! a relocation, verified by `just schema-hash`, which fingerprints the generated SQL surface
//! with pgrx's non-reproducible ordering normalised away (E21).
//!
//! The state is a `bytea` carrying `UnitSum` plus the ISO code, because a fold through `+`
//! cannot widen (R2-F4b). `PARALLEL = SAFE` rests on the combine function, which is here too.

use super::{currency_or_error, describe, kmoney};
use kamu_money_core::arith::UnitSum;
use pgrx::prelude::*;

// =========================================================================================
// `sum(kmoney)` — the row aggregate, restored with a WIDE transition state.
//
// R2-F4 removed a `sum(kmoney)` whose transition state was a `kmoney`. That state re-checked the
// domain on every partial total, so `[MAX, MAX, -MAX]` succeeded or failed depending on the order
// PostgreSQL combined rows — and `PARALLEL = SAFE` made the order a planner decision. The removal
// was correct; the aggregate was genuinely wrong.
//
// What it left behind was `kmoney_sum(VARIADIC array_agg(col))` as the only way to total a column,
// and that is not a row aggregate at all: it materialises every value into one PostgreSQL array,
// iterates it, and allocates a second vector — memory linear in the number of rows, on a type
// whose whole purpose is ledger columns. A reconciliation over a large table was the one shape
// this extension could not do.
//
// So the state is widened instead of the operation being deleted. `UnitSum` is the same kernel
// `kmoney_sum` and Rust's `Money::try_sum` use: the domain is enforced per TERM as each row
// arrives, accumulation is `I256`, and the domain is checked ONCE in the final function. Merging
// is associative and commutative, so the total belongs to the multiset rather than to the plan —
// which is what makes `PARALLEL = SAFE` honest here where it was a lie before.
//
// `kmoney_sum(VARIADIC)` stays. It is the explicit-values form, the analogue of `try_sum` taking
// values rather than a column, and nothing about a working row aggregate makes it redundant.
//
// `kmoney_mixed` still gets NOTHING — see C8 below. `SELECT sum(amount)` on a mixed column must
// keep failing when the statement is PLANNED, before a row is read.
// =========================================================================================

/// Width of the `sum(kmoney)` transition state: a wide accumulator plus the ISO numeric code.
const SUM_STATE_BYTES: usize = UnitSum::ENCODED_BYTES + 2;

/// Encode a transition state.
///
/// `bytea` rather than a bespoke SQL type or `internal`. `internal` would need a serialize /
/// deserialize pair before `PARALLEL = SAFE` could be declared, and it would put raw pointers into
/// an aggregate memory context; a bespoke type would add a catalog entry whose text form is
/// meaningless money. A plain `bytea` state is copied into the aggregate context by PostgreSQL,
/// which frees the previous one, so memory stays constant in the number of rows.
fn sum_state_encode(acc: UnitSum, code: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(SUM_STATE_BYTES);
    out.extend_from_slice(&acc.to_le_bytes());
    out.extend_from_slice(&code.to_le_bytes());
    out
}

/// Decode a transition state, refusing anything that is not one.
///
/// The state type is `bytea`, so these functions are callable by hand with arbitrary bytes. A
/// forged state must be a SQL error rather than a misread of whatever was passed — this is the
/// same reasoning as the binary `RECEIVE` path, which validates rather than trusts its payload.
fn sum_state_decode(state: &[u8], context: &str) -> (UnitSum, u16) {
    let Ok(bytes) = <[u8; SUM_STATE_BYTES]>::try_from(state) else {
        error!("{context}: transition state must be exactly {SUM_STATE_BYTES} bytes, got {}", state.len());
    };
    let (units, code) = bytes.split_at(UnitSum::ENCODED_BYTES);
    (
        UnitSum::from_le_bytes(units.try_into().expect("split_at(ENCODED_BYTES) yields ENCODED_BYTES")),
        u16::from_le_bytes(code.try_into().expect("SUM_STATE_BYTES - ENCODED_BYTES == 2")),
    )
}

/// `sum(kmoney)` transition function: fold one row into the wide accumulator.
///
/// Non-strict, which is required rather than incidental: with no `INITCOND` the state arrives NULL
/// for the first row, and a strict function would never be called to establish it.
// `Vec<u8>` by value: pgrx's `#[pg_extern]` ABI takes owned argument types to build the SQL
// wrapper, so `needless_pass_by_value` cannot be honoured here.
#[allow(clippy::needless_pass_by_value)]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_sum_accum(state: Option<Vec<u8>>, value: Option<kmoney>) -> Option<Vec<u8>> {
    // A NULL row leaves the state exactly as it was, so an all-NULL group finishes NULL rather
    // than a currencyless zero. This is what `sum()` does for every built-in type, and what
    // `kmoney_sum`'s `flatten()` does for the variadic form.
    let Some(value) = value else {
        return state;
    };

    let (acc, code) = match state {
        // First non-NULL row: it names the currency for the rest of the group.
        None => (UnitSum::ZERO, value.code()),
        Some(bytes) => {
            let (acc, code) = sum_state_decode(&bytes, "sum(kmoney)");
            if code != value.code() {
                // The fastest check available, a raw `u16` compare, and the same rule `+` and
                // `kmoney_sum` apply. A column holding two currencies has no total.
                let (left, right) = (describe(code), describe(value.code()));
                error!("kmoney: cannot sum {left} and {right}: different currencies");
            }
            (acc, code)
        }
    };

    let acc = acc.add_units(value.units()).unwrap_or_else(|e| error!("sum(kmoney): {e}"));
    Some(sum_state_encode(acc, code))
}

/// `sum(kmoney)` combine function: merge two partial states from parallel workers.
///
/// Either side may be NULL — a worker that scanned no rows produces no state — so this is
/// non-strict too.
#[allow(clippy::needless_pass_by_value)]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_sum_combine(left: Option<Vec<u8>>, right: Option<Vec<u8>>) -> Option<Vec<u8>> {
    let (left, right) = match (left, right) {
        (None, other) | (other, None) => return other,
        (Some(l), Some(r)) => (l, r),
    };

    let (acc_l, code_l) = sum_state_decode(&left, "sum(kmoney)");
    let (acc_r, code_r) = sum_state_decode(&right, "sum(kmoney)");
    if code_l != code_r {
        let (a, b) = (describe(code_l), describe(code_r));
        error!("kmoney: cannot sum {a} and {b}: different currencies");
    }

    // Associative and commutative, so it does not matter which worker finished first. That is the
    // property the removed narrow-state aggregate did not have.
    let acc = acc_l.merge(acc_r).unwrap_or_else(|e| error!("sum(kmoney): {e}"));
    Some(sum_state_encode(acc, code_l))
}

/// `sum(kmoney)` final function: one narrowing, one domain check.
#[allow(clippy::needless_pass_by_value)]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_sum_final(state: Option<Vec<u8>>) -> Option<kmoney> {
    // No rows, or every row NULL: no currency to carry, so NULL — never a currencyless zero.
    let (acc, code) = sum_state_decode(&state?, "sum(kmoney)");
    // A stored code kamu_money_core does not know is corruption, not a currency.
    let _ = currency_or_error(code, "sum(kmoney)");
    let total = acc.finish().unwrap_or_else(|e| error!("sum(kmoney): {e}"));
    Some(kmoney::new(total, code))
}

// Hand-written because pgrx's `#[pg_aggregate]` derives the transition state from a
// `PostgresType`, which is a varlena carrying a serde payload -- a per-row encode/decode this
// state does not need. The three functions above are ordinary `#[pg_extern]`s; this is the
// catalog entry that makes PostgreSQL call them in the right order.
extension_sql!(
    r"
CREATE AGGREGATE sum(kmoney) (
    SFUNC       = kmoney_sum_accum,
    STYPE       = bytea,
    COMBINEFUNC = kmoney_sum_combine,
    FINALFUNC   = kmoney_sum_final,
    PARALLEL    = SAFE
);
",
    name = "kmoney_sum_aggregate",
    requires = ["kmoney_concrete", kmoney_sum_accum, kmoney_sum_combine, kmoney_sum_final],
);

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// `sum(kmoney)` totals a COLUMN, which is what a ledger schema actually asks for.
    ///
    /// The aggregate R2-F4 removed had a `kmoney` transition state; this one is wide. What it
    /// replaces in practice is `kmoney_sum(VARIADIC array_agg(col))`, which had to materialise
    /// every row into a single array first.
    #[pg_test]
    fn the_sum_aggregate_totals_a_column() {
        Spi::run("CREATE TABLE ledger (amount kmoney)").expect("table created");
        Spi::run("INSERT INTO ledger VALUES ('USD 10.50'), ('USD 0.25'), ('USD 0.25')")
            .expect("rows inserted");
        let total =
            Spi::get_one::<String>("SELECT sum(amount)::text FROM ledger").expect("query ran").expect("row");
        assert_eq!(total, "USD 11.00");
    }

    /// A group with no rows, and a group whose every row is NULL, both total to NULL.
    ///
    /// Never a currencyless zero: there is no currency to carry, which is the same reason
    /// `kmoney_sum` of nothing is NULL.
    #[pg_test]
    fn the_sum_aggregate_of_nothing_is_null() {
        Spi::run("CREATE TABLE ledger (amount kmoney)").expect("table created");
        let empty =
            Spi::get_one::<bool>("SELECT sum(amount) IS NULL FROM ledger").expect("query ran").expect("row");
        assert!(empty, "no rows must total NULL");

        Spi::run("INSERT INTO ledger VALUES (NULL), (NULL)").expect("rows inserted");
        let all_null =
            Spi::get_one::<bool>("SELECT sum(amount) IS NULL FROM ledger").expect("query ran").expect("row");
        assert!(all_null, "an all-NULL group must total NULL");

        // And a NULL among real rows is skipped, exactly as `sum()` skips NULLs everywhere else.
        Spi::run("INSERT INTO ledger VALUES ('USD 1.00'), (NULL), ('USD 2.00')").expect("rows inserted");
        let total =
            Spi::get_one::<String>("SELECT sum(amount)::text FROM ledger").expect("query ran").expect("row");
        assert_eq!(total, "USD 3.00");
    }

    /// **The R2-F4 property, tested through the path a parallel plan actually takes.**
    ///
    /// Driving the transition and combine functions by hand is deliberate: it simulates two
    /// workers deterministically, where waiting for the planner to choose a parallel plan would
    /// make the test a statement about the planner's mood. One worker's partial (`MAX + MAX`) has
    /// LEFT the domain; the other's (`-MAX`) brings the total back inside it. The old narrow state
    /// failed on that transient, and failed differently depending on which side arrived first.
    #[pg_test]
    fn the_sum_aggregate_is_plan_independent_across_a_domain_edge_transient() {
        let max = "USD 999999999999999999.999999999999999999";
        let neg = "USD -999999999999999999.999999999999999999";

        let heavy = format!("kmoney_sum_accum(kmoney_sum_accum(NULL, '{max}'), '{max}')");
        let light = format!("kmoney_sum_accum(NULL, '{neg}')");

        for (a, b) in [(&heavy, &light), (&light, &heavy)] {
            let total = Spi::get_one::<String>(&format!(
                "SELECT kmoney_sum_final(kmoney_sum_combine({a}, {b}))::text"
            ))
            .expect("query ran")
            .expect("row");
            assert_eq!(
                total, max,
                "the total must belong to the multiset, not to the order the workers finished"
            );
        }
    }

    /// **The planner really does split this aggregate**, which the test above deliberately does
    /// not check.
    ///
    /// Those two tests look similar and prove different things, so it is worth saying which is
    /// which. The one above drives `kmoney_sum_accum` / `kmoney_sum_combine` BY HAND: that makes
    /// it deterministic, portable to `YugabyteDB`, and a statement about the KERNEL. What it cannot
    /// see is the catalog. `CREATE AGGREGATE` can be declared in ways that leave the hand-driven
    /// behaviour perfect while stopping PostgreSQL from ever choosing partial aggregation --- drop
    /// `COMBINEFUNC`, or mark the aggregate `PARALLEL UNSAFE`, and every existing test still
    /// passes while `sum(kmoney)` silently becomes serial-only. An external review named that gap
    /// (2026-07-25, M-1); this closes it.
    ///
    /// **`NOT-PORTABLE` on purpose.** This asserts a PLAN, and `YugabyteDB`'s planner need not
    /// choose the same shape as stock PostgreSQL's --- which is exactly why the portable case
    /// suite drives the functions by hand instead. Marked as such in `COVERAGE.md` rather than
    /// quietly omitted.
    ///
    /// The costs are forced to zero rather than the table made huge: the question is whether a
    /// partial plan is AVAILABLE, not what the planner picks at some particular row count.
    #[pg_test]
    fn the_planner_splits_the_sum_aggregate_and_both_plans_agree() {
        Spi::run("CREATE TABLE ledger (amount kmoney)").expect("table created");
        // Domain-edge values in the DATA, so a parallel split is exercised on the transient the
        // narrow state used to fail: any worker boundary that lands between them has a partial
        // sum outside the domain.
        Spi::run(
            "INSERT INTO ledger
             SELECT ('USD ' || (g % 7) || '.25')::kmoney FROM generate_series(1, 5000) g",
        )
        .expect("bulk rows inserted");
        Spi::run(
            "INSERT INTO ledger VALUES
                ('USD 999999999999999999.999999999999999999'),
                ('USD 999999999999999999.999999999999999999'),
                ('USD -999999999999999999.999999999999999999'),
                ('USD -999999999999999999.999999999999999999')",
        )
        .expect("edge rows inserted");
        Spi::run("ANALYZE ledger").expect("analyzed");

        // THE BASELINE IS FORCED, NOT ASSUMED. This used to compute `serial` under whatever the
        // server's defaults happened to be and then call it the serial half of a
        // serial-versus-parallel comparison. On a small table with stock costs it almost
        // certainly was serial --- and "almost certainly" is the whole defect: a server already
        // configured to favour parallel scans would run BOTH queries in parallel while the test
        // went on claiming the two plans agreed. It would then pass without ever comparing the
        // two things it is named after.
        Spi::run("SET max_parallel_workers_per_gather = 0").expect("guc set");
        let serial_plan = explain_of("SELECT sum(amount) FROM ledger");
        assert!(
            !serial_plan.contains("Partial Aggregate") && !serial_plan.contains("Finalize Aggregate"),
            "the baseline was supposed to be SERIAL and the planner split it anyway, so the \
             comparison below would be parallel-versus-parallel and would prove nothing about \
             the combine function. Plan was:\n{serial_plan}"
        );
        let serial = Spi::get_one::<String>("SELECT sum(amount)::text FROM ledger")
            .expect("serial query ran")
            .expect("row");

        for guc in [
            "SET max_parallel_workers_per_gather = 4",
            "SET max_parallel_workers = 8",
            "SET parallel_setup_cost = 0",
            "SET parallel_tuple_cost = 0",
            "SET min_parallel_table_scan_size = 0",
        ] {
            Spi::run(guc).expect("guc set");
        }

        // `EXPLAIN` returns one row per plan line, so the whole plan is joined into one string:
        // the assertion is about which NODES are present, not about their layout.
        let plan = explain_of("SELECT sum(amount) FROM ledger");
        assert!(
            plan.contains("Partial Aggregate") && plan.contains("Finalize Aggregate"),
            "the planner did not split sum(kmoney). A missing COMBINEFUNC or a PARALLEL UNSAFE \
             declaration produces exactly this while every hand-driven test still passes. Plan \
             was:\n{plan}"
        );

        let parallel = Spi::get_one::<String>("SELECT sum(amount)::text FROM ledger")
            .expect("parallel query ran")
            .expect("row");
        assert_eq!(
            serial, parallel,
            "serial and parallel totals disagree, so the total is a property of the plan --- \
             which is the R2-F4 defect, back through the catalog rather than the state width"
        );
    }

    /// `EXPLAIN` as one string. Its own function because `Spi::get_one` wants a single row and
    /// `EXPLAIN` yields one row per plan line.
    fn explain_of(query: &str) -> String {
        let mut lines = Vec::new();
        Spi::connect(|client| {
            let rows =
                client.select(&format!("EXPLAIN (COSTS OFF) {query}"), None, &[]).expect("explain ran");
            for row in rows {
                if let Ok(Some(line)) = row.get::<String>(1) {
                    lines.push(line);
                }
            }
        });
        lines.join("\n")
    }

    /// A worker that scanned no rows contributes a NULL state, and merging it changes nothing.
    #[pg_test]
    fn the_sum_aggregate_combines_an_empty_partial() {
        let one = "kmoney_sum_accum(NULL, 'USD 1.00')";
        for expr in [format!("kmoney_sum_combine({one}, NULL)"), format!("kmoney_sum_combine(NULL, {one})")] {
            let total = Spi::get_one::<String>(&format!("SELECT kmoney_sum_final({expr})::text"))
                .expect("query ran")
                .expect("row");
            assert_eq!(total, "USD 1.00");
        }
        let nothing = Spi::get_one::<bool>("SELECT kmoney_sum_final(kmoney_sum_combine(NULL, NULL)) IS NULL")
            .expect("query ran")
            .expect("row");
        assert!(nothing, "two empty partials still total NULL");
    }

    #[pg_test(error = "kmoney: cannot sum USD and IDR: different currencies")]
    fn the_sum_aggregate_refuses_a_mixed_currency_column() {
        Spi::run("CREATE TABLE ledger (amount kmoney)").expect("table created");
        Spi::run("INSERT INTO ledger VALUES ('USD 1.00'), ('IDR 1.00')").expect("rows inserted");
        Spi::get_one::<String>("SELECT sum(amount)::text FROM ledger").ok();
    }

    #[pg_test(
        error = "sum(kmoney): money domain overflow: 1000000000000000000000000000000000000 units is outside the domain |units| <= 999999999999999999999999999999999999 (NUMERIC(36,18) admits |v| < 10^18)"
    )]
    fn the_sum_aggregate_rejects_a_total_that_leaves_the_domain() {
        Spi::run("CREATE TABLE ledger (amount kmoney)").expect("table created");
        Spi::run(
            "INSERT INTO ledger VALUES ('USD 999999999999999999.999999999999999999'), \
             ('USD 0.000000000000000001')",
        )
        .expect("rows inserted");
        Spi::get_one::<String>("SELECT sum(amount)::text FROM ledger").ok();
    }

    /// The transition state is `bytea`, so the support functions are callable by hand with
    /// arbitrary bytes. That must be a SQL error, not a misread of whatever was passed — the same
    /// rule the binary `RECEIVE` path follows.
    #[pg_test(error = "sum(kmoney): transition state must be exactly 34 bytes, got 3")]
    fn the_sum_aggregate_rejects_a_forged_transition_state() {
        Spi::get_one::<String>("SELECT kmoney_sum_final('\\xdeadbe'::bytea)::text").ok();
    }

    /// The aggregate and the variadic form are the same kernel, so they must agree.
    #[pg_test]
    fn the_sum_aggregate_agrees_with_the_variadic_form() {
        Spi::run("CREATE TABLE ledger (amount kmoney)").expect("table created");
        Spi::run(
            "INSERT INTO ledger VALUES ('IDR 16000.25'), ('IDR 0.000000000000000001'), \
             ('IDR -1.50')",
        )
        .expect("rows inserted");
        let agree =
            Spi::get_one::<bool>("SELECT sum(amount) = kmoney_sum(VARIADIC array_agg(amount)) FROM ledger")
                .expect("query ran")
                .expect("row");
        assert!(agree, "the row aggregate and the variadic form are one kernel");
    }

    /// **The reason `kmoney_mixed` exists**, and it is exactly what `kmoney` no longer has.
    ///
    /// A mixed column stores and filters by equality, but cannot be summed: `sum(kmoney_mixed)`
    /// fails when the statement is *planned*, before any row is read, because no such aggregate
    /// was ever defined. That is the SQL analogue of `Add` being absent on an untyped money —
    /// not a check that runs, but an operation that never existed.
    ///
    /// `kmoney` now HAS `sum`, which sharpens rather than weakens the contrast: a pinned column
    /// can be totalled, a mixed one still cannot be, and the difference is a plan-time error
    /// rather than a row-time one. Adding `sum(kmoney_mixed)` for symmetry would destroy the
    /// only guarantee the mixed type makes.
    #[pg_test(error = "function sum(kmoney_mixed) does not exist")]
    fn sum_on_a_mixed_column_fails_at_plan_time() {
        Spi::run("CREATE TABLE payments (amount kmoney_mixed)").expect("table created");
        Spi::run("INSERT INTO payments VALUES ('USD 1.00'), ('IDR 16000.00')").expect("rows inserted");
        Spi::get_one::<String>("SELECT sum(amount)::text FROM payments").ok();
    }
}
