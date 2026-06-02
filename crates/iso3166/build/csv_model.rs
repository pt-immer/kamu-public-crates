//! CSV models.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Country {
    pub alpha2: String, // uppercase 2 chars
    pub alpha3: String, // uppercase 3 chars
    pub numeric: u16,
    pub name_short: String,
    pub name_long: String,
}

#[derive(Debug, Clone)]
pub struct Subdivision {
    pub parent: String, // Alpha2
    pub code: String,   // e.g. ID-JK
    pub name: String,
    pub language: String,
    pub parent_sub: String,
    pub category: String,
    pub local_variant: String,
}

pub fn read_countries<P: AsRef<Path>>(path: P) -> Vec<Country> {
    let path = path.as_ref();
    let mut rdr = csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .has_headers(false)
        .flexible(false)
        .from_path(path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));

    let mut out = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let r = rec.unwrap_or_else(|e| panic!("{}: row {} parse error: {e}", path.display(), i));
        if r.len() != 5 {
            panic!("{}: row {} has {} fields, expected 5", path.display(), i, r.len());
        }
        let numeric: u16 = r[2].parse().unwrap_or_else(|e| {
            panic!("{}: row {}: numeric '{}' parse: {e}", path.display(), i, &r[2])
        });
        out.push(Country {
            alpha2: r[0].to_string(),
            alpha3: r[1].to_string(),
            numeric,
            name_short: r[3].to_string(),
            name_long: r[4].to_string(),
        });
    }
    assert!(!out.is_empty(), "no countries parsed from {}", path.display());

    // Sort deterministically by numeric code.
    out.sort_by_key(|c| c.numeric);

    // Sanity: unique alpha2, alpha3, numeric.
    {
        let mut a2: Vec<_> = out.iter().map(|c| &c.alpha2).collect();
        a2.sort();
        let len = a2.len();
        a2.dedup();
        assert_eq!(a2.len(), len, "duplicate alpha2 codes");
    }
    {
        let mut a3: Vec<_> = out.iter().map(|c| &c.alpha3).collect();
        a3.sort();
        let len = a3.len();
        a3.dedup();
        assert_eq!(a3.len(), len, "duplicate alpha3 codes");
    }
    {
        let mut n: Vec<u16> = out.iter().map(|c| c.numeric).collect();
        n.sort();
        let len = n.len();
        n.dedup();
        assert_eq!(n.len(), len, "duplicate numeric codes");
    }

    // Validate format.
    for c in &out {
        assert_eq!(c.alpha2.len(), 2, "alpha2 wrong length: {}", c.alpha2);
        assert_eq!(c.alpha3.len(), 3, "alpha3 wrong length: {}", c.alpha3);
        assert!(
            c.alpha2.bytes().all(|b| b.is_ascii_uppercase()),
            "alpha2 not uppercase ASCII: {}",
            c.alpha2
        );
        assert!(
            c.alpha3.bytes().all(|b| b.is_ascii_uppercase()),
            "alpha3 not uppercase ASCII: {}",
            c.alpha3
        );
        assert!(c.numeric <= 999, "numeric out of 3-digit range: {}", c.numeric);
    }
    out
}

pub fn read_subdivisions<P: AsRef<Path>>(path: P, countries: &[Country]) -> Vec<Subdivision> {
    let path = path.as_ref();
    let mut rdr = csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .has_headers(false)
        .flexible(false)
        .from_path(path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));

    let known: std::collections::BTreeSet<&str> =
        countries.iter().map(|c| c.alpha2.as_str()).collect();

    let mut out = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let r = rec.unwrap_or_else(|e| panic!("{}: row {} parse error: {e}", path.display(), i));
        if r.len() != 7 {
            panic!("{}: row {} has {} fields, expected 7", path.display(), i, r.len());
        }
        let parent = r[0].to_string();
        if !known.contains(parent.as_str()) {
            panic!("{}: row {} has unknown parent alpha2 '{}'", path.display(), i, parent);
        }
        out.push(Subdivision {
            parent,
            code: r[1].to_string(),
            name: r[2].to_string(),
            language: r[3].to_string(),
            parent_sub: r[4].to_string(),
            category: r[5].to_string(),
            local_variant: r[6].to_string(),
        });
    }

    // The same subdivision code may appear multiple times, one row per
    // language variant. Collapse to one entry per code, preferring English
    // (`en`) when available, otherwise the first occurrence in file order.
    {
        use std::collections::BTreeMap;
        let mut first_seen: BTreeMap<String, usize> = BTreeMap::new();
        let mut english_idx: BTreeMap<String, usize> = BTreeMap::new();
        for (i, s) in out.iter().enumerate() {
            first_seen.entry(s.code.clone()).or_insert(i);
            if s.language == "en" {
                english_idx.entry(s.code.clone()).or_insert(i);
            }
        }
        let mut keep = vec![false; out.len()];
        for (code, first) in &first_seen {
            let idx = english_idx.get(code).copied().unwrap_or(*first);
            keep[idx] = true;
        }
        let mut iter = keep.iter();
        out.retain(|_| *iter.next().unwrap());
    }

    // Dedup: after collapsing, the dataset has exactly one row per
    // subdivision code. Enforce uniqueness so phf key generation does not
    // collide.
    {
        let mut codes: Vec<&str> = out.iter().map(|s| s.code.as_str()).collect();
        codes.sort();
        let len = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), len, "duplicate subdivision codes detected after dedup");
    }

    // Validate format: XX-YYY where XX matches parent, YYY is 1-3 ASCII alnum uppercase.
    for s in &out {
        let bytes = s.code.as_bytes();
        assert!(
            bytes.len() >= 4 && bytes.len() <= 6,
            "subdivision code length out of range: {}",
            s.code
        );
        assert_eq!(&s.code[..2], s.parent, "subdivision code prefix != parent: {}", s.code);
        assert_eq!(&s.code[2..3], "-", "subdivision code missing '-': {}", s.code);
        for b in &bytes[3..] {
            assert!(
                b.is_ascii_alphanumeric() && !b.is_ascii_lowercase(),
                "subdivision code non-uppercase-alnum: {}",
                s.code
            );
        }
    }

    // Sort by (parent order determined by numeric code of parent, then code).
    let parent_order: std::collections::BTreeMap<&str, u16> =
        countries.iter().map(|c| (c.alpha2.as_str(), c.numeric)).collect();
    out.sort_by(|a, b| {
        let pa = parent_order[a.parent.as_str()];
        let pb = parent_order[b.parent.as_str()];
        pa.cmp(&pb).then_with(|| a.code.cmp(&b.code))
    });
    out
}
