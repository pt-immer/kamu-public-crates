//! `serde` integration for ISO 3166 types.
//!
//! Serialization formats:
//!   - [`Alpha2`] and [`Alpha3`] serialize as their canonical uppercase string.
//!   - [`Numeric`] serializes as a raw `u16` (not zero-padded).
//!   - [`Category`] serializes as the upstream raw string (e.g. `"PROVINCE"`).
//!   - [`Subdivision`] serializes as a struct; deserializes from the
//!     canonical code string (`"ID-JK"`) via a phf lookup into the static
//!     table.

use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::one::{Alpha2, Alpha3, Numeric};
use crate::two::{Category, Subdivision};

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
                crate::two::category_from_known_str(v).ok_or_else(|| {
                    de::Error::custom(
                        "unknown ISO 3166-2 subdivision category; `Category::Other` \
                         variants cannot be deserialized because the crate stores no \
                         owned strings",
                    )
                })
            }
        }
        d.deserialize_str(V)
    }
}

impl Serialize for Subdivision {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Subdivision", 7)?;
        st.serialize_field("parent", &self.parent)?;
        st.serialize_field("code", &self.code)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("language", &self.language)?;
        st.serialize_field("parent_subdivision", &self.parent_subdivision)?;
        st.serialize_field("category", &self.category)?;
        st.serialize_field("local_variant", &self.local_variant)?;
        st.end()
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
