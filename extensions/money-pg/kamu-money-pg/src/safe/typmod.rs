//! Safe parsing, rendering, and coercion for `kmoney('IDR')`.

use core::ffi::CStr;

use kamu_money_core::Iso4217;
use pgrx::prelude::*;

use super::describe;
use super::payload::{ValidationError, validate_payload};
use crate::kmoney;

/// Parse one PostgreSQL type-modifier token into an ISO numeric code.
pub(crate) fn parse(raw: &CStr) -> i32 {
    let Ok(alpha3) = raw.to_str() else {
        error!("kmoney: type modifier is not valid UTF-8");
    };
    let Some(currency) = Iso4217::from_alpha3(alpha3) else {
        error!("kmoney: {alpha3:?} is not an ISO 4217 code kamu_money_core knows");
    };
    i32::from(currency.numeric())
}

/// Render a stored ISO numeric code for `format_type` and `pg_dump`.
pub(crate) fn render(typmod: i32) -> String {
    let Some(currency) = u16::try_from(typmod).ok().and_then(Iso4217::from_numeric) else {
        error!("kmoney: stored type modifier {typmod} is not an ISO 4217 numeric code")
    };
    format!("('{}')", currency.alpha3())
}

/// Check a value against a column's declared currency.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_coerce(value: kmoney, typmod: i32, _is_explicit: bool) -> kmoney {
    if typmod == -1 {
        let _ = super::validated_or_error(value.payload(), "kmoney");
        return value;
    }

    let Some(expected) = u16::try_from(typmod).ok().and_then(Iso4217::from_numeric) else {
        error!("kmoney: column type modifier {typmod} is not an ISO 4217 numeric code");
    };
    let amount = validate_payload(value.payload(), Some(expected)).unwrap_or_else(|error| match error {
        ValidationError::UnexpectedCurrency { found_code, .. } => error!(
            "kmoney: column is declared kmoney('{}') but the value is {}",
            expected.alpha3(),
            describe(found_code)
        ),
        ValidationError::UnknownCurrency { .. } | ValidationError::OutOfDomain { .. } => {
            error!("kmoney: {error}")
        }
    });
    kmoney::from_payload(amount.payload())
}

// PostgreSQL invokes this length-coercion cast for typmod-bearing columns.
extension_sql!(
    r"
CREATE CAST (kmoney AS kmoney) WITH FUNCTION kmoney_coerce(kmoney, integer, boolean) AS IMPLICIT;
",
    name = "kmoney_typmod_cast",
    requires = ["kmoney_concrete", kmoney_coerce],
);

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn a_typmod_column_round_trips_its_currency() {
        Spi::run("CREATE TABLE pinned (amount kmoney('IDR'))").expect("table created");
        Spi::run("INSERT INTO pinned VALUES ('IDR 16000.00')").expect("row inserted");

        let stored =
            Spi::get_one::<String>("SELECT amount::text FROM pinned").expect("query ran").expect("not null");
        assert_eq!(stored, "IDR 16000.00");

        let declared = Spi::get_one::<String>(
            "SELECT format_type(atttypid, atttypmod) FROM pg_attribute
              WHERE attrelid = 'pinned'::regclass AND attname = 'amount'",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(declared, "kmoney('IDR')", "typmod_out must round-trip for pg_dump");
    }

    #[pg_test(error = "kmoney: column is declared kmoney('IDR') but the value is USD")]
    fn a_typmod_column_refuses_the_wrong_currency() {
        Spi::run("CREATE TABLE pinned_reject (amount kmoney('IDR'))").expect("table created");
        Spi::get_one::<String>("INSERT INTO pinned_reject VALUES ('USD 1.00')").ok();
    }

    #[pg_test]
    fn an_unpinned_column_still_accepts_every_currency() {
        Spi::run("CREATE TABLE unpinned (amount kmoney)").expect("table created");
        Spi::run("INSERT INTO unpinned VALUES ('USD 1.00'), ('IDR 16000.00')").expect("rows inserted");
        let n = Spi::get_one::<i64>("SELECT count(*) FROM unpinned").expect("query ran").expect("not null");
        assert_eq!(n, 2);
    }

    #[pg_test(error = "kmoney: \"ZWL\" is not an ISO 4217 code kamu_money_core knows")]
    fn a_typmod_of_an_unknown_currency_is_refused() {
        Spi::run("CREATE TABLE bad_typmod (amount kmoney('ZWL'))").ok();
    }

    #[pg_test(error = "kmoney: expected exactly one type modifier, as in kmoney('IDR'); got 2")]
    fn two_type_modifiers_are_refused() {
        Spi::run("CREATE TABLE two_mods (amount kmoney('IDR', 'USD'))").ok();
    }

    #[pg_test(error = "kmoney: cannot compute IDR + USD: different currencies")]
    fn typmod_does_not_reach_operators_so_the_value_check_still_fires() {
        Spi::run("CREATE TABLE lhs (amount kmoney('IDR'))").expect("table created");
        Spi::run("CREATE TABLE rhs (amount kmoney('USD'))").expect("table created");
        Spi::run("INSERT INTO lhs VALUES ('IDR 1.00')").expect("row inserted");
        Spi::run("INSERT INTO rhs VALUES ('USD 1.00')").expect("row inserted");
        Spi::get_one::<String>("SELECT (l.amount + r.amount)::text FROM lhs l, rhs r").ok();
    }
}
