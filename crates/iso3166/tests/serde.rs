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
fn subdivision_deserializes_from_code() {
    let sub: Subdivision = serde_json::from_str("\"ID-JK\"").unwrap();
    assert_eq!(sub.parent, Alpha2::ID);
    assert_eq!(sub.code, "ID-JK");
}

#[test]
fn subdivision_serializes_as_struct() {
    let sub: Subdivision = *Subdivision::try_from_str("ID-JK").unwrap();
    let j = serde_json::to_string(&sub).unwrap();
    assert!(j.contains("\"parent\":\"ID\""));
    assert!(j.contains("\"code\":\"ID-JK\""));
}
