//! Safe semantic layer over the raw PostgreSQL payload and ABI.

pub(crate) mod payload;
pub(crate) mod pinned;
pub(crate) mod typmod;

use kamu_money_core::Iso4217;
use pgrx::prelude::*;

use crate::{kmoney, kmoney_mixed};
use payload::{Payload, ValidatedAmount, ValidationError, validate_payload};

mod aggregate;
mod allocation;
mod division;
mod mixed;
mod ops;

/// Require matching, assigned currencies before an operation.
pub(crate) fn same_currency(a: kmoney, b: kmoney, op: &str) -> Iso4217 {
    if a.code() != b.code() {
        let (left, right) = (describe(a.code()), describe(b.code()));
        error!("kmoney: cannot compute {left} {op} {right}: different currencies");
    }
    let left = validated_for_operation(a, op);
    let _ = validated_for_operation(b, op);
    left.currency()
}

/// Resolve a stored code without inventing a currency for corrupt bytes.
pub(crate) fn currency_or_error(code: u16, context: &str) -> Iso4217 {
    Iso4217::from_numeric(code).unwrap_or_else(|| {
        error!("{context}: stored ISO 4217 numeric code {code} is not in kamu_money_core's table")
    })
}

/// Describe an ISO code while constructing another error.
pub(crate) fn describe(code: u16) -> String {
    Iso4217::from_numeric(code)
        .map_or_else(|| format!("<unknown code {code}>"), |currency| currency.alpha3().to_owned())
}

/// Validate a stored payload before safe semantic code consumes it.
pub(crate) fn validated_or_error(payload: Payload, context: &str) -> ValidatedAmount {
    validate_payload(payload, None).unwrap_or_else(|error| error!("{context}: {error}"))
}

fn validated_for_operation(value: kmoney, op: &str) -> ValidatedAmount {
    validate_payload(value.payload(), None).unwrap_or_else(|error| match error {
        ValidationError::OutOfDomain { currency, .. } => error!(
            "kmoney: a stored {} value is outside the domain |units| <= 10^36 - 1 \
             and cannot be used with {op}",
            currency.alpha3()
        ),
        ValidationError::UnexpectedCurrency { .. } | ValidationError::UnknownCurrency { .. } => {
            error!("kmoney: {error}")
        }
    })
}
