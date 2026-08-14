//! The default form, nameable so a field can say so explicitly.
//!
//! `{"currency":"USD","amount":"10.50"}` / `{"base":"USD","quote":"IDR","rate":"16000"}`.
//! Identical to the bare `Serialize`/`Deserialize` impls; this module exists so a struct
//! mixing both modes reads symmetrically instead of leaving one field's format implicit.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Serialize in the default structured form.
///
/// # Errors
/// Propagates the serializer's own errors.
pub fn serialize<T: Serialize, S: Serializer>(value: &T, s: S) -> Result<S::Ok, S::Error> {
    value.serialize(s)
}

/// Deserialize from the default structured form.
///
/// # Errors
/// Propagates the deserializer's own errors, including the currency cross-check.
pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
    T::deserialize(d)
}
