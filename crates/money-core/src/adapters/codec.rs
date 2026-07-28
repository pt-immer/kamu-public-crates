use core::fmt::Display;
use core::str::FromStr;

pub(super) fn encode<T: Display>(value: &T) -> String {
    value.to_string()
}

pub(super) fn decode<T: FromStr>(text: &str) -> Result<T, T::Err> {
    text.parse()
}
