//! Build-time codegen for kamu-iso3166.
//!
//! Reads vendored CSVs under `vendor/iso3166-csv/` and emits static Rust
//! sources into `$OUT_DIR`. Emits:
//!   - `country_generated.rs` — ISO 3166-1 types, tables, phf maps.
//!   - `subdivision_generated.rs` — ISO 3166-2 subdivisions + categories.

#![allow(clippy::too_many_lines, clippy::stable_sort_primitive, clippy::manual_assert)]
#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf};

#[path = "build/codegen_country.rs"]
mod codegen_country;
#[path = "build/codegen_subdivision.rs"]
mod codegen_subdivision;
#[path = "build/csv_model.rs"]
mod csv_model;

fn main() {
    println!("cargo:rerun-if-changed=vendor/iso3166-csv/countries.csv");
    println!("cargo:rerun-if-changed=vendor/iso3166-csv/subdivisions.csv");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/csv_model.rs");
    println!("cargo:rerun-if-changed=build/codegen_country.rs");
    println!("cargo:rerun-if-changed=build/codegen_subdivision.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));

    let countries = csv_model::read_countries("vendor/iso3166-csv/countries.csv");
    let subdivisions = csv_model::read_subdivisions("vendor/iso3166-csv/subdivisions.csv", &countries);

    let country = codegen_country::emit(&countries);
    write_generated(&out_dir.join("country_generated.rs"), country);

    let subdivision = codegen_subdivision::emit(&countries, &subdivisions);
    write_generated(&out_dir.join("subdivision_generated.rs"), subdivision);
}

fn write_generated(path: &std::path::Path, tokens: proc_macro2::TokenStream) {
    let syntax =
        syn::parse2(tokens).unwrap_or_else(|error| panic!("parse generated {}: {error}", path.display()));
    let source = prettyplease::unparse(&syntax);
    fs::write(path, source).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}
