//! Generates `kamu-money-core`'s ISO 4217 register at build time, from the maintenance agency's
//! published list.
//!
//! **Internal to this crate.** The emitted code names `crate::currency::StaticCurrency` and
//! `crate::currency::private::Sealed`, so it only compiles inside `kamu-money-core`. That is
//! sound — `private` is `pub(crate)`, so nothing leaks and no foreign crate can forge a
//! currency. It is also why this was collapsed out of a separate published crate: as a
//! `#[proc_macro]` on crates.io it could never have been used by anyone, because invoking it
//! anywhere else fails with `cannot find 'currency' in 'crate'`.
//!
//! # Why generated, and not a committed table
//!
//! The currency register is **data**, not code. It has 178 entries, it changes on ISO's
//! schedule rather than this project's, and the field most likely to be wrong — the minor-unit
//! exponent — is the one that decides how money is rendered and settled. A table typed by hand
//! reviews as correct and settles amounts wrongly.
//!
//! Deriving it here means `vendor/list-one.xml` is the only place a currency fact exists. There
//! is no generated file to regenerate, no verifier to forget to run, and no way to hand-edit a
//! row: the source and the table are the same object. This is the same shape `kamu-iso3166`
//! uses for its ISO 3166 tables.
//!
//! # What this emits
//!
//! One [`generate`] call produces everything the register implies:
//!
//! - `pub enum Iso4217`, `#[repr(u16)]`, one variant per currency, discriminant = ISO numeric
//! - `numeric`, `alpha3`, `exponent`, `name`, `from_numeric`, `from_alpha3`, `EVERY`
//! - one ZST per currency (`pub struct USD;`) with `StaticCurrency` and `Sealed` impls
//!
//! `src/iso.rs` pulls the result in with `include!`, inside a `generated` module that relaxes
//! two pedantic lints. That relaxation is the one real cost of generating from a build script
//! rather than a proc macro: expanded macro output is largely exempt from clippy, whereas an
//! `include!`d file is ordinary source and every lint applies to it.
//!
//! # Validation is a build failure, not a test
//!
//! The register is checked as it is read, and a violation fails the build of every crate that
//! depends on this one. That is stronger than a test, which can be skipped, and stronger than a
//! script, which can be forgotten:
//!
//! - a currency appearing under several countries must agree with itself in every row
//! - no two currencies may share a numeric code
//! - no name may contain a character that would not survive a Rust string literal
//! - every alpha-3 must be three ASCII uppercase letters, and a legal Rust identifier
//!
//! The fixture tests covering those failure paths live in `tests/register_codegen.rs`, which
//! pulls this module in with `#[path]` — a build script is not a test target, so without that
//! they would never run.

#![deny(missing_docs)]
#![deny(clippy::all, clippy::pedantic)]
// The `cargo_common_metadata` allow that stood here is GONE, for the reason kamu-money-core
// records: it existed because `repository` was absent, and every crate manifest carries one now.
// An exemption whose stated condition has stopped being true is worse than no exemption -- it
// suppresses a real finding while its comment tells the reader why that is fine.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::BTreeMap;

/// The register itself, embedded at the time THIS crate is compiled.
///
/// `include_str!` rather than a runtime read: a proc macro runs in the consumer's build, whose
/// working directory is not this crate's, so a path lookup would be both non-hermetic and
/// dependent on where cargo happened to invoke it.
const REGISTER: &str = include_str!("../vendor/list-one.xml");

/// The edition `list-one.xml` is pinned to — the machine-readable manifest.
///
/// Provenance used to live in prose and in a lone `reg.len() == 178` assertion.
/// Between them they could not tell a replaced file from the recorded one: a different edition
/// with the same number of codes and the same handful of spot rows passed every check while the
/// documented checksum and publication date silently described something else. The test was
/// named `the_vendored_register_is_the_edition_it_claims_to_be`, which is a stronger claim than
/// a row count can support.
///
/// Every field below is now checked against the file itself — the date and the three counts
/// while the build script parses, as build failures; the digest in a test, because hashing needs a
/// dependency the build does not otherwise want. `VENDORED.md` is checked against these
/// constants too, so the human-readable credit cannot drift from the machine-checked facts.
///
/// These are private to this module and are the single definition: the emitter, the tests and
/// the documentation check all read here.
mod edition {
    /// The file's own `Pblshd` attribute.
    pub const PUBLISHED: &str = "2026-01-01";
    /// SHA-256 of `list-one.xml`, lowercase hex.
    ///
    /// `allow(dead_code)` because only the test suite reads it — the build cannot hash without
    /// a dependency it does not otherwise need. It lives here anyway: splitting one provenance
    /// fact away from the other four is how they drift apart.
    #[allow(dead_code)]
    pub const SHA256: &str = "838dfb991648cf36df939edd5fe3811737962b75a32252847d239cedd1e291c9";
    /// Total `<CcyNtry>` elements, including territories with no currency of their own.
    pub const CCYNTRY_ROWS: usize = 280;
    /// Of those, the rows that carry a `<Ccy>`.
    pub const ROWS_WITH_CCY: usize = 277;
    /// Distinct currency codes, which is what the register actually contains.
    pub const DISTINCT_CODES: usize = 178;
}

/// Check the parsed file against [`edition`]. `Err` becomes a build-script panic.
///
/// Deliberately NOT folded into [`parse_register`]: that function is exercised by fixture
/// documents of two or three rows, and an edition check inside it would fail every one of them.
/// Separating the two also separates the questions — "is this a well-formed register?" and "is
/// it the register we recorded?" are different failures with different fixes.
fn validate_edition(xml: &str, distinct_codes: usize) -> Result<(), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not parseable as XML: {e}"))?;

    let published = doc.root_element().attribute("Pblshd").ok_or("the register has no Pblshd attribute")?;
    if published != edition::PUBLISHED {
        return Err(format!(
            "register says Pblshd=\"{published}\" but this crate records \
             \"{}\"; update VENDORED.md and the edition manifest together",
            edition::PUBLISHED
        ));
    }

    let rows: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("CcyNtry")).collect();
    let with_ccy = rows.iter().filter(|n| n.children().any(|c| c.has_tag_name("Ccy"))).count();

    for (label, actual, recorded) in [
        ("CcyNtry rows", rows.len(), edition::CCYNTRY_ROWS),
        ("rows carrying a Ccy", with_ccy, edition::ROWS_WITH_CCY),
        ("distinct codes", distinct_codes, edition::DISTINCT_CODES),
    ] {
        if actual != recorded {
            return Err(format!(
                "register has {actual} {label}, manifest records {recorded}; if the file was \
                 replaced on purpose, update VENDORED.md and the edition manifest together"
            ));
        }
    }
    Ok(())
}

/// One currency, as published.
#[derive(Debug, PartialEq, Eq)]
struct Currency {
    numeric: u16,
    /// `None` for the codes with no minor unit at all — metals, funds, test codes.
    exponent: Option<u8>,
    name: String,
}

/// Expand the ISO 4217 register into an enum, its lookups, and one ZST per currency.
///
/// Takes no arguments. The register is fixed at this crate's compile time;
/// parameterising it would imply a choice that does not exist.
///
/// This was a `#[proc_macro]` in a separate published crate. It could never have been used by
/// anyone: the tokens it emits name `crate::currency::StaticCurrency` and
/// `crate::currency::private::Sealed`, so it only ever compiled inside `kamu-money-core`.
/// Generating from a build script instead deletes a crates.io package nobody could depend on,
/// and matches how `kamu-iso3166` builds its tables from vendored data.
///
/// A `panic!` here replaces the `compile_error!` the macro emitted. Cargo prints a build
/// script's panic message, and the failure reaches every crate downstream, so the property
/// that matters -- a bad register cannot be skipped past with `--skip` -- is unchanged.
pub(crate) fn generate() -> TokenStream {
    match parse_register(REGISTER).and_then(|c| {
        // The edition gate runs AFTER the shape gate, so a mangled file reports what is wrong
        // with it rather than "the counts do not match" -- which is true of a corrupted file
        // and tells whoever replaced it nothing useful.
        validate_edition(REGISTER, c.len())?;
        Ok(c)
    }) {
        Ok(c) => emit(&c),
        Err(message) => panic!("ISO 4217 register is invalid: {message}"),
    }
}

/// Turn a validated register into the tokens `kamu-money-core` compiles.
///
/// Split out of [`iso4217_register`] so the entry point is just "read, validate, emit" and
/// this is just the emission -- clippy's line limit was the prompt, but the seam is real.
#[allow(clippy::too_many_lines)] // one `quote!` block; splitting it would hide the shape
fn emit(currencies: &BTreeMap<String, Currency>) -> TokenStream {
    let variants = currencies.iter().map(|(code, c)| {
        let ident = format_ident!("{}", code);
        let numeric = c.numeric;
        let name = &c.name;
        quote! { #[doc = #name] #ident = #numeric }
    });

    let alpha3_arms = currencies.keys().map(|code| {
        let ident = format_ident!("{}", code);
        quote! { Self::#ident => #code }
    });

    let exponent_arms = currencies.iter().map(|(code, c)| {
        let ident = format_ident!("{}", code);
        // Fully qualified: `quote!` stamps `Span::call_site()`, so a bare `Some`/`None`
        // resolves in the CONSUMER's scope. A module that shadows `Option` would break
        // expansion with an error naming neither this macro nor the cause.
        let value = c.exponent.map_or_else(
            || quote!(::core::option::Option::None),
            |e| quote!(::core::option::Option::Some(#e)),
        );
        quote! { Self::#ident => #value }
    });

    let name_arms = currencies.iter().map(|(code, c)| {
        let ident = format_ident!("{}", code);
        let name = &c.name;
        quote! { Self::#ident => #name }
    });

    let from_numeric_arms = currencies.iter().map(|(code, c)| {
        let ident = format_ident!("{}", code);
        let numeric = c.numeric;
        quote! { #numeric => ::core::option::Option::Some(Self::#ident) }
    });

    let from_alpha3_arms = currencies.keys().map(|code| {
        let ident = format_ident!("{}", code);
        quote! { #code => ::core::option::Option::Some(Self::#ident) }
    });

    let every = currencies.keys().map(|code| {
        let ident = format_ident!("{}", code);
        quote! { Self::#ident }
    });

    let zsts = currencies.iter().map(|(code, c)| {
        let ident = format_ident!("{}", code);
        let name = &c.name;
        quote! {
            #[doc = #name]
            ///
            /// Named for its ISO alpha-3 code, not `UpperCamelCase`. The code IS the name; a
            /// separate `Usd`-style spelling would be a second name for a thing that already
            /// has one. It also makes `USD` and `Iso4217::USD` agree.
            #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
            pub struct #ident;

            impl crate::currency::StaticCurrency for #ident {
                const CODE: Iso4217 = Iso4217::#ident;
            }

            impl crate::currency::private::Sealed for #ident {}
        }
    });

    let count = currencies.len();
    let count_doc = format!(
        "The register is **complete**: all {count} codes of ISO 4217, generated at compile \
         time from the maintenance agency's published list rather than typed by hand."
    );

    quote! {
        /// An ISO 4217 currency. Closed set: an unknown code is a parse error, never a silent pass.
        ///
        #[doc = #count_doc]
        ///
        /// `#[non_exhaustive]` stays, because completeness is a fact about a *date*, not a
        /// property. ISO adds and withdraws codes, so a downstream `match` must carry a
        /// wildcard arm or the next currency to exist is a breaking change for every consumer.
        /// (This does not weaken the closed-set contract, which is about parsing: an
        /// unrecognized code still fails, it is never silently accepted.)
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(u16)]
        #[non_exhaustive]
        pub enum Iso4217 { #(#variants),* }

        impl Iso4217 {
            /// ISO 4217 numeric-3 code.
            // The ONE permitted `as` in the crate. The enum is `#[repr(u16)]` with explicit
            // discriminants, so this reads the discriminant exactly -- it does not narrow a
            // value, and cannot lose one. `mem::discriminant` returns an opaque type and cannot
            // produce the number, so there is no non-`as` alternative. (specs.md C10)
            #[allow(clippy::as_conversions)]
            #[must_use]
            pub const fn numeric(self) -> u16 { self as u16 }

            /// ISO 4217 alpha-3 code.
            #[must_use]
            pub const fn alpha3(self) -> &'static str {
                match self { #(#alpha3_arms),* }
            }

            /// ISO 4217 **settlement** exponent. `None` for metals/funds/test codes
            /// (XAU, XDR, XXX...) which genuinely have no minor unit.
            ///
            /// This is NOT the display exponent. IDR settles at 2 and displays at 0.
            #[must_use]
            pub const fn exponent(self) -> ::core::option::Option<u8> {
                match self { #(#exponent_arms),* }
            }

            /// English entity name, as published.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self { #(#name_arms),* }
            }

            /// Parse a numeric-3 code.
            #[must_use]
            pub const fn from_numeric(n: u16) -> ::core::option::Option<Self> {
                match n { #(#from_numeric_arms,)* _ => ::core::option::Option::None }
            }

            /// Parse an alpha-3 code. Case-sensitive; ISO codes are uppercase.
            ///
            /// Not `const`: `&str` equality is not const-stable (`PartialEq` is not yet a const
            /// trait, rust-lang/rust#143874), and `match` on `str` is not allowed in a const fn.
            #[must_use]
            pub fn from_alpha3(s: &str) -> ::core::option::Option<Self> {
                // A `match` on `&str`, not a chain of `if s == "..."`. The chain was inherited
                // from the const-fn era and cost up to 178 sequential string comparisons per
                // lookup; `match` lets rustc build a length-and-first-byte decision tree. The
                // "not const" note above still applies -- it is about `PartialEq`, not about
                // which construct the body uses.
                match s {
                    #(#from_alpha3_arms,)*
                    _ => ::core::option::Option::None,
                }
            }

            /// Every currency in the register.
            ///
            /// **Not `ALL`.** `ALL` is the Albanian lek, so an associated const by that name is
            /// shadowed by its own variant once the register is complete -- the path
            /// `Iso4217::ALL` stops meaning "every currency" and starts meaning "Lek". It
            /// surfaces as `` `Iso4217` is not an iterator ``, which is a confusing way to be
            /// told a constant was overwritten by data, and it cannot happen while a table
            /// holds only a handful of hand-picked codes.
            ///
            /// The general shape: an associated item sharing a namespace with
            /// externally-defined identifiers will eventually collide, because the register
            /// grows and the names in it are not ours to choose.
            ///
            /// Ordered by **alpha-3 code**, not by the numeric discriminant that `Ord` on
            /// `Iso4217` compares — so this is deliberately NOT sorted per its own `Ord`, and
            /// `binary_search` over it would be wrong. Iterate it.
            pub const EVERY: &'static [Iso4217] = &[ #(#every),* ];
        }

        #(#zsts)*
    }
}

/// Parse and validate the register. `Err` becomes a `compile_error!` at the call site.
///
/// `BTreeMap` rather than `HashMap`: expansion must be deterministic, or the emitted variant
/// order changes between builds and every downstream diff becomes noise.
fn parse_register(xml: &str) -> Result<BTreeMap<String, Currency>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("not parseable as XML: {e}"))?;

    let text = |node: roxmltree::Node<'_, '_>, tag: &str| -> Option<String> {
        node.children().find(|c| c.has_tag_name(tag)).map(|c| {
            // Every text child, not just the first. roxmltree splits element text across
            // several nodes when entities or CDATA appear, so `Node::text()` alone would
            // silently truncate a future `CcyNm` containing `&amp;` or `&#233;` rather
            // than failing. Today's file has none; that is not a guarantee about the next
            // edition.
            c.children().filter_map(|t| t.text()).collect::<String>().trim().to_owned()
        })
    };

    let mut register: BTreeMap<String, Currency> = BTreeMap::new();
    let mut numerics: BTreeMap<u16, String> = BTreeMap::new();

    for entry in doc.descendants().filter(|n| n.has_tag_name("CcyNtry")) {
        // Territories with no currency of their own carry no <Ccy> at all. Skipping them is
        // correct; treating a missing code as an error would reject the published file.
        let Some(code) = text(entry, "Ccy") else {
            continue;
        };

        let numeric: u16 = text(entry, "CcyNbr")
            .ok_or_else(|| format!("{code} has no numeric code"))?
            .parse()
            .map_err(|e| format!("{code} has an unparsable numeric code: {e}"))?;

        let minor = text(entry, "CcyMnrUnts").ok_or_else(|| format!("{code} has no minor-unit field"))?;
        let exponent = if minor == "N.A." {
            None
        } else {
            Some(minor.parse::<u8>().map_err(|e| format!("{code} has an unparsable minor-unit count: {e}"))?)
        };

        let name = text(entry, "CcyNm").unwrap_or_default();

        if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(format!("{code} is not three ASCII uppercase letters"));
        }
        // ISO numeric-3 is 0..=999 and the widest minor unit ISO uses is 4 (CLF). kamu-money-core's
        // tests asserted both -- downstream of the code that should have refused them, which
        // makes a bad register a failing test instead of a failing build.
        if numeric > 999 {
            return Err(format!("{code}'s numeric code {numeric} is not a 3-digit ISO code"));
        }
        if let Some(e) = exponent
            && e > 4
        {
            return Err(format!("{code} claims {e} minor digits; ISO's widest is 4"));
        }
        if name.is_empty() {
            return Err(format!("{code} has no name"));
        }
        if name.contains('"') || name.contains('\\') {
            return Err(format!("{code}'s name would not survive a string literal: {name}"));
        }

        let parsed = Currency { numeric, exponent, name };

        // A currency used in several countries appears once per country. Every such row must
        // agree, or the table would depend on which row happened to be read first.
        if let Some(seen) = register.get(&code) {
            if seen.numeric != parsed.numeric || seen.exponent != parsed.exponent || seen.name != parsed.name
            {
                return Err(format!("{code} disagrees with itself across country rows"));
            }
            continue;
        }

        // Two currencies sharing a numeric code would make `from_numeric` lossy, and the binary
        // serde form encodes exactly that number.
        if let Some(other) = numerics.get(&numeric) {
            return Err(format!("numeric {numeric} is claimed by both {other} and {code}"));
        }
        numerics.insert(numeric, code.clone());
        register.insert(code, parsed);
    }

    if register.is_empty() {
        return Err("no currencies found -- is the vendored list-one.xml intact?".to_owned());
    }
    Ok(register)
}

#[cfg(test)]
mod tests {
    use super::{Currency, parse_register};

    /// A register with `rows` spliced in, so each fixture states only what it is testing.
    fn xml(rows: &str) -> String {
        format!(r#"<?xml version="1.0"?><ISO_4217 Pblshd="2026-01-01"><CcyTbl>{rows}</CcyTbl></ISO_4217>"#)
    }

    fn row(country: &str, name: &str, code: &str, num: &str, minor: &str) -> String {
        format!(
            "<CcyNtry><CtryNm>{country}</CtryNm><CcyNm>{name}</CcyNm>\
             <Ccy>{code}</Ccy><CcyNbr>{num}</CcyNbr><CcyMnrUnts>{minor}</CcyMnrUnts></CcyNtry>"
        )
    }

    #[test]
    fn a_currency_used_in_several_countries_is_read_once() {
        let doc = xml(&format!(
            "{}{}",
            row("GERMANY", "Euro", "EUR", "978", "2"),
            row("FRANCE", "Euro", "EUR", "978", "2")
        ));
        let reg = parse_register(&doc).unwrap();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg["EUR"], Currency { numeric: 978, exponent: Some(2), name: "Euro".to_owned() });
    }

    /// The check that matters most: the register lists one row per COUNTRY, so a currency
    /// disagreeing with itself would make the table depend on which row parsed first.
    #[test]
    fn a_currency_that_disagrees_with_itself_is_refused() {
        let doc = xml(&format!(
            "{}{}",
            row("GERMANY", "Euro", "EUR", "978", "2"),
            row("FRANCE", "Euro", "EUR", "978", "3")
        ));
        let err = parse_register(&doc).unwrap_err();
        assert!(err.contains("disagrees with itself"), "{err}");
    }

    #[test]
    fn two_currencies_may_not_share_a_numeric_code() {
        let doc =
            xml(&format!("{}{}", row("A", "Alpha", "AAA", "111", "2"), row("B", "Beta", "BBB", "111", "2")));
        let err = parse_register(&doc).unwrap_err();
        assert!(err.contains("claimed by both"), "{err}");
    }

    /// Territories with no currency of their own (Antarctica and two more) carry no `<Ccy>`.
    /// Skipping them is correct; erroring would reject the published file.
    #[test]
    fn an_entry_with_no_currency_is_skipped_not_refused() {
        let doc = xml(&format!(
            "<CcyNtry><CtryNm>ANTARCTICA</CtryNm></CcyNtry>{}",
            row("A", "Alpha", "AAA", "111", "2")
        ));
        let reg = parse_register(&doc).unwrap();
        assert_eq!(reg.len(), 1);
        assert!(reg.contains_key("AAA"));
    }

    #[test]
    fn no_minor_unit_means_no_exponent_not_zero() {
        let doc = xml(&row("X", "Gold", "XAU", "959", "N.A."));
        assert_eq!(parse_register(&doc).unwrap()["XAU"].exponent, None);
    }

    #[test]
    fn malformed_rows_are_refused_one_reason_at_a_time() {
        let cases: &[(String, &str)] = &[
            (xml(&row("A", "Alpha", "aaa", "111", "2")), "uppercase"),
            (xml(&row("A", "Alpha", "AAAA", "111", "2")), "uppercase"),
            (xml(&row("A", "Alpha", "AAA", "1111", "2")), "3-digit"),
            (xml(&row("A", "Alpha", "AAA", "111", "9")), "minor digits"),
            (xml(&row("A", "", "AAA", "111", "2")), "no name"),
            (xml(&row("A", "Alpha", "AAA", "abc", "2")), "unparsable numeric"),
            (xml(&row("A", "Alpha", "AAA", "111", "x")), "unparsable minor"),
            (xml(""), "no currencies found"),
        ];
        for (doc, expected) in cases {
            let err = parse_register(doc).unwrap_err();
            assert!(err.contains(expected), "expected {expected:?}, got {err:?}");
        }
    }

    #[test]
    fn a_document_that_is_not_xml_is_refused() {
        let err = parse_register("this is not xml").unwrap_err();
        assert!(err.contains("not parseable as XML"), "{err}");
    }

    /// The one check that can tell a replaced file from the recorded one.
    ///
    /// Counts and spot rows cannot: a different edition with 178 codes and the same USD row
    /// passes every other test here. A digest is the only assertion that fails when the bytes
    /// change for any reason at all, which is why the crate carries a test-only `sha2` for it.
    #[test]
    fn the_vendored_register_hashes_to_its_recorded_digest() {
        use core::fmt::Write as _;
        use sha2::{Digest, Sha256};
        // Written out rather than `format!("{:x}", digest)`: sha2 0.11 returns a `hybrid_array`
        // that no longer implements `LowerHex`, which 0.10's `generic-array` output did.
        let actual = Sha256::digest(super::REGISTER.as_bytes()).iter().fold(
            String::with_capacity(64),
            |mut acc, b| {
                let _ = write!(acc, "{b:02x}");
                acc
            },
        );
        assert_eq!(
            actual,
            super::edition::SHA256,
            "list-one.xml is not the file this crate records. If it was replaced deliberately, \
             update VENDORED.md and the edition manifest together; if it was not, the register \
             has been modified in place, which is the failure this test exists for."
        );
    }

    /// The edition gate the macro runs at compile time, exercised here too so its failure paths
    /// are known to work rather than assumed to. A compile error cannot be unit-tested directly.
    #[test]
    fn the_edition_gate_accepts_the_real_file_and_refuses_a_substitute() {
        let reg = parse_register(super::REGISTER).unwrap();
        super::validate_edition(super::REGISTER, reg.len()).expect("the vendored file passes");

        // Right shape, wrong edition: every structural rule holds and the counts do not.
        let substitute = xml(&row("A", "Alpha", "AAA", "111", "2"));
        let err = super::validate_edition(&substitute, 1).unwrap_err();
        assert!(err.contains("CcyNtry rows"), "{err}");

        // Right counts, wrong date -- the case a count-only check waves through.
        let wrong_date = super::REGISTER
            .replace(&format!("Pblshd=\"{}\"", super::edition::PUBLISHED), "Pblshd=\"2027-01-01\"");
        let err = super::validate_edition(&wrong_date, reg.len()).unwrap_err();
        assert!(err.contains("Pblshd"), "{err}");
    }

    /// `VENDORED.md` is the human-readable credit, and it went stale: it recorded "281 country
    /// rows" against a file with 280, of which 277 carry a code. Nothing read the prose, so
    /// nothing caught it — the same failure mode as the control file that kept describing a
    /// renamed type. The document must now quote the machine-checked numbers.
    /// Each value must appear IN ITS OWN LABELLED ROW, not merely somewhere in the file.
    ///
    /// A bare `doc.contains("280")` looked equivalent and is not — measured, while mutating the
    /// manifest to check this test bites. The document discusses the old wrong count in prose
    /// ("this table read 178, from 281 country rows"), so a `contains` search for the mutated
    /// 281 found that sentence and passed. The test would have reported agreement with a
    /// provenance table stating the opposite of the manifest.
    ///
    /// Anchoring on the row label is what makes the assertion about the table.
    #[test]
    fn the_credit_document_agrees_with_the_machine_checked_manifest() {
        let doc = include_str!("../VENDORED.md");
        let row_states = |label: &str, value: &str| {
            doc.lines().filter(|l| l.starts_with('|') && l.contains(label)).any(|l| {
                // Split the row into cells so the value must BE a cell, not sit inside a
                // longer number: "178" is a substring of "1780".
                l.split('|').any(|cell| cell.trim().trim_matches('*').trim_matches('`') == value)
            })
        };

        for (label, value) in [
            ("Published", super::edition::PUBLISHED.to_owned()),
            ("SHA-256", super::edition::SHA256.to_owned()),
            ("`<CcyNtry>` rows", super::edition::CCYNTRY_ROWS.to_string()),
            ("carrying a `<Ccy>`", super::edition::ROWS_WITH_CCY.to_string()),
            ("Distinct currency codes", super::edition::DISTINCT_CODES.to_string()),
        ] {
            assert!(
                row_states(label, &value),
                "VENDORED.md's \"{label}\" row does not state {value}, which is what the \
                 edition manifest records. The credit and the data have drifted apart."
            );
        }
    }

    /// The real register, pinned where the data lives rather than in the consumer crate.
    #[test]
    fn the_vendored_register_has_the_settlement_exponents_it_should() {
        let reg = parse_register(super::REGISTER).unwrap();
        assert_eq!(reg.len(), super::edition::DISTINCT_CODES);
        assert_eq!(reg["USD"].numeric, 840);
        assert_eq!(reg["USD"].exponent, Some(2));
        assert_eq!(reg["JPY"].exponent, Some(0));
        assert_eq!(reg["KWD"].exponent, Some(3));
        assert_eq!(reg["CLF"].exponent, Some(4));
        assert_eq!(reg["XAU"].exponent, None);
        // ALL is the Albanian lek, which collided with the `Iso4217::ALL` const and forced it
        // to become `EVERY`. Named here so the collision has a test, not just a comment.
        assert_eq!(reg["ALL"].name, "Lek");
    }
}
