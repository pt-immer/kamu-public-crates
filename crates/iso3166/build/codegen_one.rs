//! ISO 3166-1 code generator.

use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use std::str::FromStr;

use crate::csv_model::Country;

pub fn emit(countries: &[Country]) -> TokenStream {
    let alpha2_variants = countries.iter().map(|c| {
        let id = ident(&c.alpha2);
        let num = Literal::u16_unsuffixed(c.numeric);
        quote! { #id = #num }
    });

    let alpha3_variants = countries.iter().map(|c| {
        let id = ident(&c.alpha3);
        let num = Literal::u16_unsuffixed(c.numeric);
        quote! { #id = #num }
    });

    // Alpha2 <-> Alpha3 via match.
    let a2_to_a3 = countries.iter().map(|c| {
        let a2 = ident(&c.alpha2);
        let a3 = ident(&c.alpha3);
        quote! { Alpha2::#a2 => Alpha3::#a3 }
    });
    let a3_to_a2 = countries.iter().map(|c| {
        let a2 = ident(&c.alpha2);
        let a3 = ident(&c.alpha3);
        quote! { Alpha3::#a3 => Alpha2::#a2 }
    });

    // Numeric -> enum via match.
    let num_to_a2 = countries.iter().map(|c| {
        let a2 = ident(&c.alpha2);
        let n = Literal::u16_unsuffixed(c.numeric);
        quote! { #n => ::core::option::Option::Some(Alpha2::#a2) }
    });
    let num_to_a3 = countries.iter().map(|c| {
        let a3 = ident(&c.alpha3);
        let n = Literal::u16_unsuffixed(c.numeric);
        quote! { #n => ::core::option::Option::Some(Alpha3::#a3) }
    });

    // Canonical str representation.
    let a2_as_str = countries.iter().map(|c| {
        let id = ident(&c.alpha2);
        let s = &c.alpha2;
        quote! { Alpha2::#id => #s }
    });
    let a3_as_str = countries.iter().map(|c| {
        let id = ident(&c.alpha3);
        let s = &c.alpha3;
        quote! { Alpha3::#id => #s }
    });

    // Metadata.
    let a2_short_name = countries.iter().map(|c| {
        let id = ident(&c.alpha2);
        let s = &c.name_short;
        quote! { Alpha2::#id => #s }
    });
    let a2_official_name = countries.iter().map(|c| {
        let id = ident(&c.alpha2);
        let s = &c.name_long;
        quote! { Alpha2::#id => #s }
    });

    // ALL slice.
    let a2_all = countries.iter().map(|c| {
        let id = ident(&c.alpha2);
        quote! { Alpha2::#id }
    });
    let a3_all = countries.iter().map(|c| {
        let id = ident(&c.alpha3);
        quote! { Alpha3::#id }
    });

    // Count constant.
    let count = countries.len();
    let count_lit = Literal::usize_unsuffixed(count);
    let a2_phf_values =
        countries.iter().map(|c| format!("Alpha2::{}", c.alpha2)).collect::<Vec<_>>();
    let a3_phf_values =
        countries.iter().map(|c| format!("Alpha3::{}", c.alpha3)).collect::<Vec<_>>();

    // phf maps: uppercase &str key -> Alpha2 / Alpha3 variants.
    let a2_phf = {
        let mut b: phf_codegen::Map<&str> = phf_codegen::Map::new();
        for (c, value) in countries.iter().zip(&a2_phf_values) {
            b.entry(c.alpha2.as_str(), value);
        }
        let s = b.build().to_string();
        TokenStream::from_str(&s).expect("phf_codegen alpha2 output parses")
    };
    let a3_phf = {
        let mut b: phf_codegen::Map<&str> = phf_codegen::Map::new();
        for (c, value) in countries.iter().zip(&a3_phf_values) {
            b.entry(c.alpha3.as_str(), value);
        }
        let s = b.build().to_string();
        TokenStream::from_str(&s).expect("phf_codegen alpha3 output parses")
    };

    quote! {
        /// ISO 3166-1 alpha-2 country code.
        ///
        /// Discriminant is the ISO 3166-1 numeric code. `Alpha2 as u16` yields
        /// that numeric value directly.
        #[allow(non_camel_case_types, clippy::upper_case_acronyms, missing_docs)]
        #[repr(u16)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Alpha2 {
            #(#alpha2_variants,)*
        }

        /// ISO 3166-1 alpha-3 country code.
        ///
        /// Discriminant is the ISO 3166-1 numeric code. `Alpha3 as u16` yields
        /// that numeric value directly.
        #[allow(non_camel_case_types, clippy::upper_case_acronyms, missing_docs)]
        #[repr(u16)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Alpha3 {
            #(#alpha3_variants,)*
        }

        impl Alpha2 {
            /// All assigned ISO 3166-1 alpha-2 codes, sorted by numeric code.
            pub const ALL: &'static [Alpha2] = &[ #(#a2_all),* ];

            /// Total number of assigned codes.
            pub const COUNT: usize = #count_lit;

            #[doc(hidden)]
            pub(crate) const fn as_str_generated(self) -> &'static str {
                match self { #(#a2_as_str,)* }
            }

            #[doc(hidden)]
            pub(crate) const fn short_name_generated(self) -> &'static str {
                match self { #(#a2_short_name,)* }
            }

            #[doc(hidden)]
            pub(crate) const fn official_name_generated(self) -> &'static str {
                match self { #(#a2_official_name,)* }
            }

            #[doc(hidden)]
            pub(crate) const fn to_alpha3_generated(self) -> Alpha3 {
                match self { #(#a2_to_a3,)* }
            }
        }

        impl Alpha3 {
            /// All assigned ISO 3166-1 alpha-3 codes, sorted by numeric code.
            pub const ALL: &'static [Alpha3] = &[ #(#a3_all),* ];

            /// Total number of assigned codes.
            pub const COUNT: usize = #count_lit;

            #[doc(hidden)]
            pub(crate) const fn as_str_generated(self) -> &'static str {
                match self { #(#a3_as_str,)* }
            }

            #[doc(hidden)]
            pub(crate) const fn to_alpha2_generated(self) -> Alpha2 {
                match self { #(#a3_to_a2,)* }
            }
        }

        #[doc(hidden)]
        pub(crate) const fn numeric_to_alpha2_generated(n: u16) -> ::core::option::Option<Alpha2> {
            match n {
                #(#num_to_a2,)*
                _ => ::core::option::Option::None,
            }
        }

        #[doc(hidden)]
        pub(crate) const fn numeric_to_alpha3_generated(n: u16) -> ::core::option::Option<Alpha3> {
            match n {
                #(#num_to_a3,)*
                _ => ::core::option::Option::None,
            }
        }

        #[doc(hidden)]
        pub(crate) static ALPHA2_BY_STR: ::phf::Map<&'static str, Alpha2> = #a2_phf;

        #[doc(hidden)]
        pub(crate) static ALPHA3_BY_STR: ::phf::Map<&'static str, Alpha3> = #a3_phf;
    }
}

fn ident(s: &str) -> Ident {
    Ident::new(s, Span::call_site())
}
