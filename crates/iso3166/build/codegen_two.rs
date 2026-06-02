//! ISO 3166-2 code generator.

use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use std::collections::BTreeMap;
use std::str::FromStr;

use crate::csv_model::{Country, Subdivision};

pub fn emit(countries: &[Country], subs: &[Subdivision]) -> TokenStream {
    // Collect distinct categories, assign CamelCase variant names.
    let mut cats: BTreeMap<String, String> = BTreeMap::new();
    for s in subs {
        if !cats.contains_key(&s.category) {
            let vname = to_camel_case(&s.category);
            // Guarantee uniqueness (should already be, but be defensive).
            let mut final_name = vname.clone();
            let mut i = 2usize;
            while cats.values().any(|v| v == &final_name) {
                final_name = format!("{vname}{i}");
                i += 1;
            }
            cats.insert(s.category.clone(), final_name);
        }
    }

    let category_variants = cats.values().map(|v| {
        let id = ident(v);
        quote! { #id }
    });

    // Category variant -> raw upstream string.
    let cat_as_str_arms = cats.iter().map(|(raw, v)| {
        let raw_lit = raw.as_str();
        let id = ident(v);
        quote! { Category::#id => #raw_lit }
    });

    // Known-category string -> Category (no Other fallback).
    let cat_from_known_arms = cats.iter().map(|(raw, v)| {
        let raw_lit = raw.as_str();
        let id = ident(v);
        quote! { #raw_lit => ::core::option::Option::Some(Category::#id) }
    });

    // Build ALL subdivisions array.
    let sub_entries = subs.iter().map(|s| {
        let parent = ident(&s.parent);
        let code = s.code.as_str();
        let name = s.name.as_str();
        let lang = s.language.as_str();
        let cat_variant = ident(&cats[&s.category]);
        let parent_sub_opt = if s.parent_sub.is_empty() {
            quote! { ::core::option::Option::None }
        } else {
            let ps = s.parent_sub.as_str();
            quote! { ::core::option::Option::Some(#ps) }
        };
        let local_variant_opt = if s.local_variant.is_empty() {
            quote! { ::core::option::Option::None }
        } else {
            let lv = s.local_variant.as_str();
            quote! { ::core::option::Option::Some(#lv) }
        };
        quote! {
            Subdivision {
                parent: crate::one::Alpha2::#parent,
                code: #code,
                name: #name,
                language: #lang,
                parent_subdivision: #parent_sub_opt,
                category: Category::#cat_variant,
                local_variant: #local_variant_opt,
            }
        }
    });

    let total = subs.len();
    let total_lit = Literal::usize_unsuffixed(total);
    let sub_phf_values =
        subs.iter().enumerate().map(|(i, _)| format!("{i}usize")).collect::<Vec<_>>();

    // Per-country offsets (subdivisions are already sorted by parent order).
    // Emit a match arm per Alpha2 with its (start, end) slice.
    let mut offsets: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let cursor = 0usize;
    let mut current: Option<&str> = None;
    for (i, s) in subs.iter().enumerate() {
        match current {
            Some(p) if p == s.parent.as_str() => {}
            _ => {
                if let Some(p) = current {
                    let start = offsets[p].0;
                    offsets.insert(p, (start, i));
                }
                offsets.insert(s.parent.as_str(), (i, i));
                current = Some(s.parent.as_str());
            }
        }
        let _ = cursor;
        let _ = i;
    }
    if let Some(p) = current {
        let start = offsets[p].0;
        offsets.insert(p, (start, subs.len()));
    }

    let subs_by_country_arms = countries.iter().map(|c| {
        let a2 = ident(&c.alpha2);
        let (start, end) = *offsets.get(c.alpha2.as_str()).unwrap_or(&(0, 0));
        let s = Literal::usize_unsuffixed(start);
        let e = Literal::usize_unsuffixed(end);
        quote! { crate::one::Alpha2::#a2 => &ALL_SUBDIVISIONS[#s..#e] }
    });

    // phf map: subdivision code -> index into ALL_SUBDIVISIONS
    let sub_phf = {
        let mut b: phf_codegen::Map<&str> = phf_codegen::Map::new();
        for (s, value) in subs.iter().zip(&sub_phf_values) {
            b.entry(s.code.as_str(), value);
        }
        let text = b.build().to_string();
        TokenStream::from_str(&text).expect("phf_codegen subdivisions output parses")
    };

    quote! {
        /// Category of an ISO 3166-2 subdivision (`province`, `state`, etc.).
        ///
        /// Variants are generated from the upstream data at build time. New
        /// categories introduced upstream surface as [`Category::Other`]. The
        /// enum is `#[non_exhaustive]`; downstream `match` expressions must
        /// include a wildcard arm.
        #[non_exhaustive]
        #[allow(non_camel_case_types, missing_docs)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Category {
            #(#category_variants,)*
            /// Unknown or future-added category. Holds the raw upstream string.
            Other(&'static str),
        }

        impl Category {
            /// Return the upstream raw string for this category.
            ///
            /// For [`Category::Other`], returns the wrapped string.
            #[must_use]
            pub const fn as_str(&self) -> &'static str {
                match self {
                    #(#cat_as_str_arms,)*
                    Category::Other(s) => s,
                }
            }
        }

        #[doc(hidden)]
        #[allow(dead_code)]
        pub(crate) fn category_from_known_str_generated(raw: &str) -> ::core::option::Option<Category> {
            match raw {
                #(#cat_from_known_arms,)*
                _ => ::core::option::Option::None,
            }
        }

        /// Compiled list of every ISO 3166-2 subdivision at the pinned upstream
        /// commit, sorted by (parent country numeric code, subdivision code).
        pub const ALL_SUBDIVISIONS: &[Subdivision] = &[ #(#sub_entries,)* ];

        /// Total number of subdivisions.
        pub const SUBDIVISION_COUNT: usize = #total_lit;

        #[doc(hidden)]
        #[allow(clippy::match_same_arms)]
        pub(crate) fn subdivisions_of_generated(a: crate::one::Alpha2) -> &'static [Subdivision] {
            match a { #(#subs_by_country_arms,)* }
        }

        #[doc(hidden)]
        pub(crate) static SUBDIVISION_BY_CODE: ::phf::Map<&'static str, usize> = #sub_phf;
    }
}

fn to_camel_case(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut capitalize = true;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            if capitalize {
                out.extend(ch.to_uppercase());
                capitalize = false;
            } else {
                out.extend(ch.to_lowercase());
            }
        } else {
            capitalize = true;
        }
    }
    if out.is_empty() || out.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn ident(s: &str) -> Ident {
    Ident::new(s, Span::call_site())
}
