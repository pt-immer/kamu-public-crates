//! `Iso4217` on the wire: alpha-3 for humans, the ISO numeric for binary.

use crate::iso::Iso4217;
use core::fmt;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Hand-written because derived binary enum representations follow variant order, not ISO
/// numeric discriminants. Human-readable form uses alpha-3; binary form uses numeric-3.
impl Serialize for Iso4217 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() { s.serialize_str(self.alpha3()) } else { s.serialize_u16(self.numeric()) }
    }
}

impl<'de> Deserialize<'de> for Iso4217 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct CodeVisitor;

        impl Visitor<'_> for CodeVisitor {
            type Value = Iso4217;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an ISO 4217 alpha-3 code or numeric-3 code")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Iso4217, E> {
                Iso4217::from_alpha3(v).ok_or_else(|| E::custom(format_args!("unknown ISO 4217 code {v:?}")))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Iso4217, E> {
                u16::try_from(v)
                    .ok()
                    .and_then(Iso4217::from_numeric)
                    .ok_or_else(|| E::custom(format_args!("unknown ISO 4217 numeric code {v}")))
            }
        }

        if d.is_human_readable() { d.deserialize_str(CodeVisitor) } else { d.deserialize_u16(CodeVisitor) }
    }
}

#[cfg(test)]
mod tests {
    use crate::iso::Iso4217;

    // ---------------------------------------------------------------------------------------
    // Serde derives binary enum values from variant position, not numeric discriminants.
    // ---------------------------------------------------------------------------------------

    /// Binary must carry the ISO **numeric** code, which a standards body assigns permanently —
    /// never the variant's ordinal position, which moves the moment a currency is inserted
    /// mid-table. The register is generated in alpha-3 order, so new codes can shift later variants.
    ///
    /// A JSON suite cannot catch this — human-readable formats emit the NAME. That is why this
    /// test is binary, and why it is the most important one in the file.
    #[test]
    fn binary_encodes_the_iso_numeric_never_the_variant_position() {
        let encoded = postcard::to_allocvec(&Iso4217::IDR).unwrap();

        assert_eq!(
            encoded,
            postcard::to_allocvec(&360u16).unwrap(),
            "IDR must encode as its ISO numeric 360"
        );
        // IDR is the SECOND variant in the table, so a position-based encoding would emit 1.
        assert_ne!(
            encoded,
            postcard::to_allocvec(&1u16).unwrap(),
            "must not be the ordinal position — that is the silent-corruption bug"
        );
        assert_eq!(postcard::from_bytes::<Iso4217>(&encoded).unwrap(), Iso4217::IDR);
    }
    #[test]
    fn human_readable_uses_the_alpha3_code_with_no_rename_all_mangling() {
        // `SCREAMING_SNAKE_CASE` would incorrectly emit "I_D_R".
        assert_eq!(serde_json::to_string(&Iso4217::IDR).unwrap(), r#""IDR""#);
        assert_eq!(serde_json::from_str::<Iso4217>(r#""IDR""#).unwrap(), Iso4217::IDR);
        assert!(serde_json::from_str::<Iso4217>(r#""I_D_R""#).is_err());
        assert!(serde_json::from_str::<Iso4217>(r#""ZZZ""#).is_err());
    }
}
