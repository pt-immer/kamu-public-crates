//! `kmoney_mixed`: the deliberately arithmetic-free type, and the proof out of it.
//!
//! The type itself stays in `lib.rs` beside the shell-type SQL that declares it. What lives
//! here is its I/O, its equality, and `kmoney_from_mixed` -- the SQL twin of proving a value
//! into `Money<C>`. There is no arithmetic, and its ABSENCE is the contract: `sum(kmoney_mixed)`
//! must fail when the query is PLANNED, not after four million rows have been read.

use super::payload::{ValidationError, validate_payload};
use super::{kmoney, kmoney_mixed, validated_or_error};
use kamu_money_core::{Iso4217, text};
use pgrx::prelude::*;

#[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
fn kmoney_mixed_in(input: &core::ffi::CStr) -> kmoney_mixed {
    let text = match input.to_str() {
        Ok(t) => t,
        Err(e) => error!("kmoney_mixed: input is not valid UTF-8: {e}"),
    };
    match text::parse(text) {
        Ok((currency, units)) => kmoney_mixed::new(units, currency.numeric()),
        Err(e) => error!("kmoney_mixed: {e}, in {text:?}"),
    }
}

#[doc(hidden)]
#[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
fn kmoney_mixed_out(value: kmoney_mixed) -> alloc::ffi::CString {
    let amount = validated_or_error(value.payload(), "kmoney_mixed");
    let rendered = text::render(amount.units(), amount.currency())
        .unwrap_or_else(|e| error!("kmoney_mixed: stored amount cannot be rendered: {e}"));
    alloc::ffi::CString::new(rendered)
        .unwrap_or_else(|e| error!("kmoney_mixed: rendered form contains a NUL byte: {e}"))
}

extension_sql!(
    r"
CREATE FUNCTION kmoney_mixed_recv(internal) RETURNS kmoney_mixed
    AS 'MODULE_PATHNAME', 'kmoney_mixed_recv'
    LANGUAGE C IMMUTABLE STRICT PARALLEL SAFE;

CREATE TYPE kmoney_mixed (
    INTERNALLENGTH = 18,
    INPUT          = kmoney_mixed_in,
    OUTPUT         = kmoney_mixed_out,
    SEND           = kmoney_mixed_send,
    RECEIVE        = kmoney_mixed_recv,
    ALIGNMENT      = char,
    STORAGE        = plain
);
",
    name = "kmoney_mixed_concrete",
    requires = [kmoney_mixed_send, "money_shell_types", kmoney_mixed_in, kmoney_mixed_out],
);

// =========================================================================================
// EQUALITY FOR kmoney_mixed -- AND STILL NO ARITHMETIC, WHICH IS THE POINT
//
// `kmoney_mixed` deliberately has no `+`, `-` or `sum()`, so `SELECT sum(amount)` over such a
// column fails when the query is PLANNED rather than on the row that disagrees. That property
// is untouched here: equality is not arithmetic. "Is this the same money" has an answer for
// any two values; "what is their sum" does not, which is exactly why one is defined and the
// other is not.
//
// What this buys a mixed column: currency-aware `=`/`<>` as predicates (a sequential-scan
// filter), and a CAST to prove a currency. There is NO opclass on the mixed type, so no value
// index and no `GROUP BY`/`DISTINCT`/`UNIQUE` on amount -- the assumed usage is keying by account/txn
// rather than grouping by amount (and a mixed column cannot be summed, so grouping it aggregates
// nothing). The limitation holds regardless of that assumption.
//
// NO ORDERING, and that is deliberate rather than unfinished. A B-tree opclass would make
// `ORDER BY amount` legal on a column whose whole purpose is to hold several currencies at
// once, and the result would sort by (currency, units) while reading as though it sorted by
// value. That is the "second number claiming to be the money" shape in a result set. Order a
// mixed column by proving the currency first -- `kmoney_from_mixed(amount, 'IDR')` -- which is
// the same discipline Rust requires before `Money<C>` arithmetic.
// =========================================================================================

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_mixed_concrete"])]
#[opname(=)]
#[commutator(=)]
#[negator(<>)]
#[restrict(eqsel)]
#[join(eqjoinsel)]
fn kmoney_mixed_eq(a: kmoney_mixed, b: kmoney_mixed) -> bool {
    a.code() == b.code() && a.units() == b.units()
}

#[pg_operator(immutable, parallel_safe, requires = ["kmoney_mixed_concrete"])]
#[opname(<>)]
#[commutator(<>)]
#[negator(=)]
#[restrict(neqsel)]
#[join(neqjoinsel)]
fn kmoney_mixed_ne(a: kmoney_mixed, b: kmoney_mixed) -> bool {
    !kmoney_mixed_eq(a, b)
}

/// The stable hash of an `kmoney_mixed`, folded to `int4`. There is no hash opclass on the
/// mixed type either (amount columns are assumed not grouped by amount), so this is not an index
/// support function -- it is retained only to prove the payload contract: a value stored as
/// `kmoney` and the same value stored as `kmoney_mixed` have identical payloads, so they must
/// hash identically. Same specified algorithm as [`kmoney_hash`], deliberately the same
/// function; the pinned-hash test asserts the two agree.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_mixed_concrete"])]
fn kmoney_mixed_hash(value: kmoney_mixed) -> i32 {
    let amount = validated_or_error(value.payload(), "kmoney_mixed");
    kamu_money_core::advanced::stable_hash::fold_to_i32(kamu_money_core::advanced::stable_hash::stable_hash(
        amount.currency().numeric(),
        amount.units(),
    ))
}

/// `kmoney_mixed -> kmoney`, **checked**: the SQL twin of proving a value into `Money<C>`.
///
/// Deliberately not an implicit cast. An implicit one would let the planner slip a mixed
/// column into `sum()` on its own initiative, which is the exact failure `kmoney_mixed`
/// exists to make impossible.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete", "kmoney_mixed_concrete"])]
fn kmoney_from_mixed(value: kmoney_mixed, expected: &str) -> kmoney {
    let Some(want) = Iso4217::from_alpha3(expected) else {
        error!("kmoney: {expected:?} is not an ISO 4217 code kamu_money_core knows");
    };
    let amount = validate_payload(value.payload(), Some(want)).unwrap_or_else(|error| match error {
        ValidationError::OutOfDomain { currency, .. } => error!(
            "kmoney: stored {} value is outside the domain |units| <= 10^36 - 1 \
             and cannot be converted from kmoney_mixed",
            currency.alpha3()
        ),
        ValidationError::UnexpectedCurrency { .. } | ValidationError::UnknownCurrency { .. } => {
            error!("kmoney: {error}")
        }
    });
    kmoney::from_payload(amount.payload())
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use super::{kmoney_from_mixed, kmoney_mixed};
    use pgrx::prelude::*;

    /// A mixed column has currency-aware equality as a PREDICATE, and STILL cannot be summed —
    /// which is what the type exists for. There is no opclass on the mixed type (amount columns are assumed not
    /// grouped or indexed by amount, and a mixed column cannot be summed so grouping it would
    /// aggregate nothing), so `=` filters but does not go through an index. The no-arithmetic
    /// half is asserted by [`sum_on_a_mixed_column_fails_at_plan_time`], which would go red if
    /// equality had dragged arithmetic in with it.
    #[pg_test]
    fn a_mixed_column_equality_is_currency_aware_and_never_raises() {
        Spi::run("CREATE TABLE mixed_eq (amount kmoney_mixed)").expect("table created");
        Spi::run("INSERT INTO mixed_eq VALUES ('USD 1.00'), ('IDR 1.00'), ('USD 1.00'), ('USD 2.00')")
            .expect("rows inserted");

        // Equality is currency-aware and total: it never raises across currencies.
        let same = Spi::get_one::<bool>("SELECT 'USD 1.00'::kmoney_mixed = 'IDR 1.00'::kmoney_mixed")
            .expect("query ran")
            .expect("row");
        assert!(!same, "same number, different currency, different money");

        // `=` works as a plain predicate (a sequential-scan filter) without any opclass.
        let n = Spi::get_one::<i64>("SELECT count(*) FROM mixed_eq WHERE amount = 'USD 1.00'::kmoney_mixed")
            .expect("query ran")
            .expect("row");
        assert_eq!(n, 2, "two USD 1.00 rows match; IDR 1.00 and USD 2.00 do not");

        // The plan-time refusal of arithmetic is UNCHANGED and asserted by
        // `sum_on_a_mixed_column_fails_at_plan_time`, not here (a failing statement would abort
        // this transaction).
    }

    /// No B-tree opclass on the mixed type, deliberately: ordering a column that holds several
    /// currencies would sort by (currency, units) while reading as though it sorted by value.
    #[pg_test(error = "operator does not exist: kmoney_mixed < kmoney_mixed")]
    fn a_mixed_column_cannot_be_ordered() {
        Spi::get_one::<bool>("SELECT 'USD 1.00'::kmoney_mixed < 'USD 2.00'::kmoney_mixed").ok();
    }

    #[pg_test(error = "operator does not exist: kmoney_mixed + kmoney_mixed")]
    fn addition_on_the_mixed_type_does_not_exist_either() {
        Spi::get_one::<String>("SELECT ('USD 1.00'::kmoney_mixed + 'USD 1.00'::kmoney_mixed)::text").ok();
    }

    /// The mixed type stores and renders perfectly well — it is only arithmetic it lacks.
    #[pg_test]
    fn a_mixed_column_stores_several_currencies_side_by_side() {
        Spi::run("CREATE TABLE mixed_ok (amount kmoney_mixed)").expect("table created");
        Spi::run("INSERT INTO mixed_ok VALUES ('USD 1.00'), ('IDR 16000.00'), ('JPY 150')")
            .expect("rows inserted");

        let rendered = Spi::get_one::<String>(
            "SELECT string_agg(amount::text, ', ' ORDER BY amount::text) FROM mixed_ok",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(rendered, "IDR 16000.00, JPY 150, USD 1.00");
    }

    /// Proving a mixed value into `kmoney` is the SQL twin of proving one into a typed
    /// `Money<C>` before it may be added.
    #[pg_test]
    fn the_conversion_out_of_mixed_proves_the_currency() {
        let got = Spi::get_one::<String>("SELECT kmoney_from_mixed('USD 2.50'::kmoney_mixed, 'USD')::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(got, "USD 2.50");
    }

    #[pg_test(error = "kmoney: expected USD, found IDR")]
    fn the_conversion_out_of_mixed_refuses_the_wrong_currency() {
        Spi::get_one::<String>("SELECT kmoney_from_mixed('IDR 2.50'::kmoney_mixed, 'USD')::text").ok();
    }

    #[pg_test(
        error = "kmoney: stored USD value is outside the domain |units| <= 10^36 - 1 and cannot be converted from kmoney_mixed"
    )]
    fn the_conversion_out_of_mixed_refuses_corrupt_units() {
        kmoney_from_mixed(kmoney_mixed::new(kamu_money_core::DOMAIN_MAX + 1, 840), "USD");
    }
}
