use core::fmt::Display;
use core::str::FromStr;

pub(super) fn encode<T: Display>(value: &T) -> String {
    value.to_string()
}

pub(super) fn decode<T: FromStr>(text: &str) -> Result<T, T::Err> {
    text.parse()
}

/// Decode a money literal that may carry its tag or arrive bare.
///
/// A per-currency `kmoney_<code>` column renders **bare** digits -- its type is
/// the currency, so the text carries none. Bare input is unambiguous here for
/// the same reason: `C` is static, so the currency comes from the Rust type
/// exactly as it comes from the column type in SQL. The tagged form stays
/// accepted and stays **checked** -- a tag that names another currency is an
/// error, never a reinterpretation.
pub(super) fn decode_money<C: crate::StaticCurrency>(
    text: &str,
) -> Result<crate::Money<C>, crate::errors::ParseMoneyError> {
    if text.contains(' ') {
        text.parse()
    } else {
        let units = crate::text::parse_amount(text)?;
        Ok(crate::Money::try_from_units(units)?)
    }
}
