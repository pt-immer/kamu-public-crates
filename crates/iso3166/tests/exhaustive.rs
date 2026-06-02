//! Exhaustive invariants over the *entire* vendored ISO 3166 dataset.
//!
//! These walk every country and every subdivision at the pinned submodule
//! commit and assert total correctness of conversions, parsing, ordering and
//! the per-country partition — the heavy lifting behind the crate's coverage.

#![allow(missing_docs, clippy::missing_panics_doc)]
#![forbid(unsafe_code)]

use kamu_iso3166::subdivision::{self, ALL_SUBDIVISIONS, SUBDIVISION_COUNT};
use kamu_iso3166::{Alpha2, Alpha3, Numeric, Subdivision};

#[test]
fn alpha_counts_are_consistent() {
    assert_eq!(Alpha2::ALL.len(), Alpha2::COUNT);
    assert_eq!(Alpha3::ALL.len(), Alpha3::COUNT);
    assert_eq!(Alpha2::COUNT, Alpha3::COUNT);
    assert_eq!(Alpha2::iter().count(), Alpha2::COUNT);
    assert_eq!(Alpha3::iter().count(), Alpha3::COUNT);
}

#[test]
fn alpha2_all_is_strictly_increasing_by_numeric() {
    let mut prev: Option<u16> = None;
    for a2 in Alpha2::iter() {
        let n = a2.to_numeric().get();
        if let Some(p) = prev {
            assert!(n > p, "Alpha2::ALL not strictly increasing by numeric at {a2:?}");
        }
        prev = Some(n);
    }
}

#[test]
fn every_country_roundtrips_through_all_representations() {
    for a2 in Alpha2::iter() {
        // alpha-2 string form
        let s = a2.as_str();
        assert_eq!(s.len(), 2, "{a2:?} as_str length");
        assert!(s.bytes().all(|b| b.is_ascii_uppercase()), "{a2:?} not ASCII uppercase");
        assert_eq!(Alpha2::try_from_str(s), Ok(a2));
        assert_eq!(Alpha2::try_from_str(&s.to_ascii_lowercase()), Ok(a2));
        assert_eq!(Alpha2::try_from_bytes(s.as_bytes()), Ok(a2));
        assert_eq!(s.parse::<Alpha2>(), Ok(a2));
        assert_eq!(Alpha2::try_from(s), Ok(a2));
        assert_eq!(a2.to_string(), s);
        assert_eq!(<Alpha2 as AsRef<str>>::as_ref(&a2), s);

        // names are populated
        assert!(!a2.short_name().is_empty(), "{a2:?} empty short_name");
        assert!(!a2.official_name().is_empty(), "{a2:?} empty official_name");

        // total, infallible conversions among the three representations
        let a3 = a2.to_alpha3();
        let n = a2.to_numeric();
        assert_eq!(a3.to_alpha2(), a2);
        assert_eq!(a3.to_numeric(), n);
        assert_eq!(n.to_alpha2(), Some(a2));
        assert_eq!(n.to_alpha3(), Some(a3));
        assert_eq!(u16::from(a2), n.get());
        assert_eq!(Alpha3::from(a2), a3);
        assert_eq!(Numeric::from(a2), n);

        // Numeric constructors all agree for assigned codes
        assert_eq!(Numeric::new(n.get()), Some(n));
        assert_eq!(Numeric::try_from_u16(n.get()), Ok(n));
        assert_eq!(Numeric::try_from(n.get()), Ok(n));
        assert_eq!(Alpha2::try_from(n), Ok(a2));
        assert_eq!(Alpha3::try_from(n), Ok(a3));

        // Numeric Display is the canonical zero-padded 3-digit form, and parses
        // back even with extra leading zeros
        let disp = n.to_string();
        assert_eq!(disp.len(), 3, "{n} display should be 3 digits");
        assert_eq!(Numeric::try_from_str(&disp), Ok(n));
        assert_eq!(Numeric::try_from_str(&format!("00{disp}")), Ok(n));
        assert_eq!(disp.parse::<Numeric>(), Ok(n));
    }
}

#[test]
fn every_alpha3_roundtrips_and_forwards_names() {
    for a3 in Alpha3::iter() {
        let s = a3.as_str();
        assert_eq!(s.len(), 3, "{a3:?} as_str length");
        assert!(s.bytes().all(|b| b.is_ascii_uppercase()), "{a3:?} not ASCII uppercase");
        assert_eq!(Alpha3::try_from_str(s), Ok(a3));
        assert_eq!(Alpha3::try_from_str(&s.to_ascii_lowercase()), Ok(a3));
        assert_eq!(Alpha3::try_from_bytes(s.as_bytes()), Ok(a3));
        assert_eq!(a3.to_string(), s);

        let a2 = a3.to_alpha2();
        assert_eq!(a2.to_alpha3(), a3);
        assert_eq!(a3.short_name(), a2.short_name());
        assert_eq!(a3.official_name(), a2.official_name());
        assert_eq!(Alpha2::from(a3), a2);
        assert_eq!(u16::from(a3), a3.to_numeric().get());
        assert_eq!(Numeric::from(a3), a3.to_numeric());
        assert_eq!(Alpha3::try_from(a3.to_numeric()), Ok(a3));
        assert_eq!(<Alpha3 as AsRef<str>>::as_ref(&a3), s);
    }
}

#[test]
fn subdivision_count_and_partition_hold() {
    assert_eq!(ALL_SUBDIVISIONS.len(), SUBDIVISION_COUNT);
    assert_eq!(Subdivision::iter().count(), SUBDIVISION_COUNT);

    // Per-country slices partition ALL_SUBDIVISIONS exactly — no orphans, no
    // double counting. This validates the generated per-country offsets.
    let summed: usize = Alpha2::iter().map(|c| c.subdivisions().len()).sum();
    assert_eq!(summed, SUBDIVISION_COUNT, "per-country slices do not partition ALL_SUBDIVISIONS");
}

#[test]
fn every_subdivision_is_well_formed_and_roundtrips() {
    for sd in Subdivision::iter() {
        let code = sd.code;
        assert!((4..=6).contains(&code.len()), "{code} length out of range");
        assert_eq!(&code[..2], sd.parent.as_str(), "{code} prefix != parent");
        assert_eq!(code.as_bytes()[2], b'-', "{code} missing '-' separator");
        assert!(
            code[3..].bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
            "{code} suffix is not uppercase ASCII alphanumeric",
        );

        // case-insensitive lookup returns the *same* static entry
        let looked = Subdivision::try_from_str(code).expect("code must parse");
        assert!(core::ptr::eq(looked, sd), "{code} lookup returned a different entry");
        assert_eq!(Subdivision::try_from_str(&code.to_ascii_lowercase()).unwrap().code, code);

        // it is reachable from its parent's slice
        assert!(
            sd.parent.subdivisions().iter().any(|x| x.code == code),
            "{code} not present in parent subdivisions()",
        );

        // formatting and metadata
        assert_eq!(sd.to_string(), code);
        assert_eq!(<Subdivision as AsRef<str>>::as_ref(sd), code);
        assert!(!sd.name.is_empty(), "{code} empty name");
        assert!(!sd.category.as_str().is_empty(), "{code} empty category string");
        // NB: `language` is legitimately empty for some upstream rows.
    }
}

#[test]
fn all_subdivisions_sorted_by_parent_numeric_then_code() {
    let mut prev: Option<(u16, &str)> = None;
    for sd in Subdivision::iter() {
        let key = (sd.parent.to_numeric().get(), sd.code);
        if let Some(p) = prev {
            assert!(p < key, "ALL_SUBDIVISIONS not strictly ordered at {}", sd.code);
        }
        prev = Some(key);
    }
}

#[test]
fn subdivisions_of_free_fn_matches_method() {
    for a2 in Alpha2::iter() {
        assert_eq!(subdivision::subdivisions_of(a2), a2.subdivisions());
        for sd in a2.subdivisions() {
            assert_eq!(sd.parent, a2);
        }
    }
}
