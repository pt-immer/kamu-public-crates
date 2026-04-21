//! Build-time codegen for kamu-iso3166.
//!
//! Reads vendored CSVs under `vendor/iso3166-csv/` and emits static Rust
//! sources into `$OUT_DIR`. Emits:
//!   - `one_generated.rs`   — ISO 3166-1 types, tables, phf maps.
//!   - `two_generated.rs`   — ISO 3166-2 subdivisions + `Category` enum.

#![allow(clippy::too_many_lines, clippy::stable_sort_primitive, clippy::manual_assert)]
#![forbid(unsafe_code)]

use std::{env, fs, path::PathBuf};

#[path = "build/codegen_one.rs"]
mod codegen_one;
#[path = "build/codegen_two.rs"]
mod codegen_two;
#[path = "build/csv_model.rs"]
mod csv_model;

fn main() {
    println!("cargo:rerun-if-changed=vendor/iso3166-csv/countries.csv");
    println!("cargo:rerun-if-changed=vendor/iso3166-csv/subdivisions.csv");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/csv_model.rs");
    println!("cargo:rerun-if-changed=build/codegen_one.rs");
    println!("cargo:rerun-if-changed=build/codegen_two.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));

    let countries = csv_model::read_countries("vendor/iso3166-csv/countries.csv");
    let subdivisions =
        csv_model::read_subdivisions("vendor/iso3166-csv/subdivisions.csv", &countries);

    let one_ts = codegen_one::emit(&countries);
    write_formatted(&out_dir.join("one_generated.rs"), one_ts.to_string());

    let two_ts = codegen_two::emit(&countries, &subdivisions);
    write_formatted(&out_dir.join("two_generated.rs"), two_ts.to_string());
}

fn write_formatted(path: &std::path::Path, src: String) {
    fs::write(path, src).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}
