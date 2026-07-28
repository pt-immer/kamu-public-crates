#![allow(missing_docs)]
#![cfg(feature = "serde")]
#![forbid(unsafe_code)]

use kamu_iso3166::{Alpha2, Alpha3, Category, Numeric, Subdivision};

#[test]
fn alpha2_roundtrip_json() {
    let v = Alpha2::ID;
    let j = serde_json::to_string(&v).unwrap();
    assert_eq!(j, "\"ID\"");
    let back: Alpha2 = serde_json::from_str(&j).unwrap();
    assert_eq!(back, v);
    // Case-insensitive acceptance on input.
    let lower: Alpha2 = serde_json::from_str("\"id\"").unwrap();
    assert_eq!(lower, v);
}

#[test]
fn alpha3_roundtrip_json() {
    let v = Alpha3::IDN;
    let j = serde_json::to_string(&v).unwrap();
    assert_eq!(j, "\"IDN\"");
    let back: Alpha3 = serde_json::from_str(&j).unwrap();
    assert_eq!(back, v);
}

#[test]
fn numeric_roundtrip_json() {
    let n = Numeric::try_from(360u16).unwrap();
    let j = serde_json::to_string(&n).unwrap();
    assert_eq!(j, "360");
    let back: Numeric = serde_json::from_str("360").unwrap();
    assert_eq!(back, n);
    // Also accept string form.
    let back_s: Numeric = serde_json::from_str("\"0360\"").unwrap();
    assert_eq!(back_s, n);
}

#[test]
fn unknown_code_fails_to_deserialize() {
    assert!(serde_json::from_str::<Alpha2>("\"ZZ\"").is_err());
    assert!(serde_json::from_str::<Alpha3>("\"ZZZ\"").is_err());
    assert!(serde_json::from_str::<Numeric>("9999").is_err());
}

#[test]
fn category_serializes_as_raw_string() {
    // Pick any subdivision and round-trip its category.
    let s: &Subdivision = Alpha2::ID.subdivisions().first().unwrap();
    let c = s.category;
    let j = serde_json::to_string(&c).unwrap();
    assert!(j.starts_with('"') && j.ends_with('"'));
    let back: Category = serde_json::from_str(&j).unwrap();
    assert_eq!(back, c);
}

#[test]
fn subdivision_roundtrips_as_canonical_code() {
    let sub: Subdivision = *Subdivision::try_from_str("ID-JK").unwrap();
    let j = serde_json::to_string(&sub).unwrap();
    assert_eq!(j, "\"ID-JK\"");
    assert_eq!(serde_json::from_str::<Subdivision>(&j).unwrap(), sub);
}

#[test]
fn every_country_serde_roundtrips() {
    for a2 in Alpha2::iter() {
        let j = serde_json::to_string(&a2).unwrap();
        assert_eq!(j, format!("\"{}\"", a2.as_str()));
        assert_eq!(serde_json::from_str::<Alpha2>(&j).unwrap(), a2);

        let a3 = a2.to_alpha3();
        let j3 = serde_json::to_string(&a3).unwrap();
        assert_eq!(j3, format!("\"{}\"", a3.as_str()));
        assert_eq!(serde_json::from_str::<Alpha3>(&j3).unwrap(), a3);

        let n = a2.to_numeric();
        let jn = serde_json::to_string(&n).unwrap();
        assert_eq!(jn, n.get().to_string()); // raw u16, not zero-padded
        assert_eq!(serde_json::from_str::<Numeric>(&jn).unwrap(), n);
    }
}

#[test]
fn every_subdivision_serde_roundtrips() {
    for sd in kamu_iso3166::subdivision::ALL_SUBDIVISIONS {
        let json = serde_json::to_string(sd).unwrap();
        assert_eq!(json, format!("\"{}\"", sd.code));
        assert_eq!(serde_json::from_str::<Subdivision>(&json).unwrap(), *sd);

        let category_json = serde_json::to_string(&sd.category).unwrap();
        assert_eq!(serde_json::from_str::<Category>(&category_json).unwrap(), sd.category);
    }
}

#[test]
fn unknown_category_fails_to_deserialize() {
    assert!(serde_json::from_str::<Category>("\"NO-SUCH-CATEGORY\"").is_err());
}

#[test]
fn deserialize_rejects_wrong_types_and_invalid_values() {
    // Wrong JSON type drives each `Visitor::expecting` path.
    assert!(serde_json::from_str::<Alpha2>("123").is_err());
    assert!(serde_json::from_str::<Alpha3>("123").is_err());
    assert!(serde_json::from_str::<Subdivision>("123").is_err());
    assert!(serde_json::from_str::<Numeric>("true").is_err());

    // Numeric accepts u64 / i64 / string but validates the value.
    assert!(serde_json::from_str::<Numeric>("-1").is_err()); // visit_i64, out of u16 range
    assert!(serde_json::from_str::<Numeric>("70000").is_err()); // visit_u64, > u16::MAX
    assert!(serde_json::from_str::<Numeric>("999").is_err()); // in range, unassigned
    assert!(serde_json::from_str::<Numeric>("\"abc\"").is_err()); // visit_str, not an integer

    // String forms of invalid country codes.
    assert!(serde_json::from_str::<Alpha2>("\"zzz\"").is_err());
    assert!(serde_json::from_str::<Subdivision>("\"ID-ZZ\"").is_err());
}

#[test]
fn numeric_accepts_i64_and_string_forms() {
    let n = Numeric::try_from(360u16).unwrap();
    assert_eq!(serde_json::from_str::<Numeric>("360").unwrap(), n); // u64
    // serde_json hands small signed integers to visit_i64 only for negatives;
    // a quoted value drives visit_str.
    assert_eq!(serde_json::from_str::<Numeric>("\"360\"").unwrap(), n);
}
