//! Repository-wide source policy, read as Rust rather than matched as text.
//!
//! The scan covers the excluded extension lane as well as the public workspace: a persisted hash
//! is a persisted hash wherever it is written.

use std::path::Path;

use syn::visit::Visit;

/// One construction of an unstable hasher.
#[derive(Debug, PartialEq, Eq)]
pub struct Offence {
    pub file: String,
    /// The path as written, so the report names what was found.
    pub path: String,
}

#[derive(Default)]
struct UnstableHasher {
    found: Vec<String>,
}

impl Visit<'_> for UnstableHasher {
    fn visit_path(&mut self, path: &syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|segment| segment.ident.to_string()).collect();
        if segments.windows(2).any(|pair| pair[0] == "DefaultHasher" && pair[1] == "new") {
            self.found.push(segments.join("::"));
        }
        syn::visit::visit_path(self, path);
    }
}

/// Every unstable-hasher construction in one Rust source.
///
/// Parsed rather than matched: a regex also finds the name in a comment, a string literal or a
/// doc example, and none of those construct anything.
pub fn offences_in(source: &str) -> Result<Vec<String>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut visitor = UnstableHasher::default();
    visitor.visit_file(&file);
    Ok(visitor.found)
}

/// Every unstable-hasher construction in the tracked Rust under one root.
pub fn unstable_hasher_offences(root: &Path) -> Vec<Offence> {
    let files = crate::tracked_in(root, &["*.rs"]);
    assert!(!files.is_empty(), "tracked Rust source discovery found nothing");

    let mut offences = Vec::new();
    for relative in files {
        let source = std::fs::read_to_string(root.join(&relative))
            .unwrap_or_else(|error| panic!("cannot read {relative}: {error}"));
        let found =
            offences_in(&source).unwrap_or_else(|error| panic!("{relative} is not parseable Rust: {error}"));
        offences.extend(found.into_iter().map(|path| Offence { file: relative.clone(), path }));
    }
    offences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_construction_is_found_however_it_is_spelled() {
        for source in [
            "fn f() { let _ = std::collections::hash_map::DefaultHasher::new(); }",
            "fn f() { let _ = DefaultHasher :: new (); }",
            "use std::collections::hash_map::DefaultHasher;\nfn f() { let _ = DefaultHasher::new(); }",
        ] {
            assert_eq!(1, offences_in(source).expect("parses").len(), "missed: {source}");
        }
    }

    #[test]
    fn the_name_without_a_construction_is_not_an_offence() {
        for source in [
            "//! DefaultHasher::new() is what this module refuses.\nfn f() {}",
            "fn f() { let _ = \"DefaultHasher::new()\"; }",
            "use std::collections::hash_map::DefaultHasher;\nfn f() -> Option<DefaultHasher> { None }",
        ] {
            assert!(offences_in(source).expect("parses").is_empty(), "false positive: {source}");
        }
    }
}
