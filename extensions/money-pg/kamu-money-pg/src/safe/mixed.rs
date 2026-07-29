//! I/O, equality, and checked conversion for `kmoney_mixed`.
//!
//! The type has no arithmetic or ordering surface. `sum(kmoney_mixed)` fails
//! during query planning; `kmoney_from_mixed` proves the currency before
//! returning `kmoney`.

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

// Equality is total and currency-aware. No operator class is registered, so
// these operators are predicates only: no value index, grouping, uniqueness,
// or ordering. Convert through `kmoney_from_mixed` before arithmetic or order.

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

/// Return the stable payload hash folded to `int4`.
///
/// The mixed type has no hash operator class. This function pins parity with
/// [`kmoney_hash`] for byte-identical payloads.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_mixed_concrete"])]
fn kmoney_mixed_hash(value: kmoney_mixed) -> i32 {
    let amount = validated_or_error(value.payload(), "kmoney_mixed");
    kamu_money_core::advanced::stable_hash::fold_to_i32(kamu_money_core::advanced::stable_hash::stable_hash(
        amount.currency().numeric(),
        amount.units(),
    ))
}

/// Convert `kmoney_mixed` to `kmoney` after checking the expected currency.
///
/// This is not an implicit cast; callers must make the proof visible.
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

    /// Equality is currency-aware and remains a non-indexed predicate.
    #[pg_test]
    fn a_mixed_column_equality_is_currency_aware_and_never_raises() {
        Spi::run("CREATE TABLE mixed_eq (amount kmoney_mixed)").expect("table created");
        Spi::run("INSERT INTO mixed_eq VALUES ('USD 1.00'), ('IDR 1.00'), ('USD 1.00'), ('USD 2.00')")
            .expect("rows inserted");

        let same = Spi::get_one::<bool>("SELECT 'USD 1.00'::kmoney_mixed = 'IDR 1.00'::kmoney_mixed")
            .expect("query ran")
            .expect("row");
        assert!(!same, "same number, different currency, different money");

        let n = Spi::get_one::<i64>("SELECT count(*) FROM mixed_eq WHERE amount = 'USD 1.00'::kmoney_mixed")
            .expect("query ran")
            .expect("row");
        assert_eq!(n, 2, "two USD 1.00 rows match; IDR 1.00 and USD 2.00 do not");
    }

    /// Mixed values have no cross-currency ordering.
    #[pg_test(error = "operator does not exist: kmoney_mixed < kmoney_mixed")]
    fn a_mixed_column_cannot_be_ordered() {
        Spi::get_one::<bool>("SELECT 'USD 1.00'::kmoney_mixed < 'USD 2.00'::kmoney_mixed").ok();
    }

    #[pg_test(error = "operator does not exist: kmoney_mixed + kmoney_mixed")]
    fn addition_on_the_mixed_type_does_not_exist_either() {
        Spi::get_one::<String>("SELECT ('USD 1.00'::kmoney_mixed + 'USD 1.00'::kmoney_mixed)::text").ok();
    }

    /// Mixed values store and render several currencies.
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

    /// Conversion succeeds when the expected currency matches.
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
