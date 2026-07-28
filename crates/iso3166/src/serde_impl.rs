//! `serde` integration for ISO 3166 types.
//!
//! Serialization formats:
//!   - [`Alpha2`] and [`Alpha3`] serialize as their canonical uppercase string.
//!   - [`Numeric`] serializes as a raw `u16` (not zero-padded).
//!   - [`Category`] serializes as the upstream raw string (e.g. `"PROVINCE"`).
//!   - [`Subdivision`] serializes and deserializes as its canonical code string
//!     (e.g. `"ID-JK"`).

use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::country::{Alpha2, Alpha3, Numeric};
use crate::subdivision::{Category, Subdivision};

impl Serialize for Alpha2 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Alpha2 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Alpha2;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("ISO 3166-1 alpha-2 country code")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Alpha2::try_from_str(v).map_err(de::Error::custom)
            }
        }
        d.deserialize_str(V)
    }
}

impl Serialize for Alpha3 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Alpha3 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Alpha3;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("ISO 3166-1 alpha-3 country code")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Alpha3::try_from_str(v).map_err(de::Error::custom)
            }
        }
        d.deserialize_str(V)
    }
}

impl Serialize for Numeric {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u16(self.get())
    }
}

impl<'de> Deserialize<'de> for Numeric {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Numeric;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("ISO 3166-1 numeric country code (u16, 0..=999)")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                let v16: u16 = v.try_into().map_err(|_| de::Error::custom("value out of range"))?;
                Numeric::try_from_u16(v16).map_err(de::Error::custom)
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                let v16: u16 = v.try_into().map_err(|_| de::Error::custom("value out of range"))?;
                Numeric::try_from_u16(v16).map_err(de::Error::custom)
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Numeric::try_from_str(v).map_err(de::Error::custom)
            }
        }
        d.deserialize_any(V)
    }
}

impl Serialize for Category {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Category {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Category;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("ISO 3166-2 subdivision category string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                crate::subdivision::category_from_known_str(v)
                    .ok_or_else(|| de::Error::custom("unknown ISO 3166-2 subdivision category"))
            }
        }
        d.deserialize_str(V)
    }
}

impl Serialize for Subdivision {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code)
    }
}

impl<'de> Deserialize<'de> for Subdivision {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = Subdivision;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("ISO 3166-2 subdivision code string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Subdivision::try_from_str(v).copied().map_err(de::Error::custom)
            }
        }
        d.deserialize_str(V)
    }
}
