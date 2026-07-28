//! Generates the ISO 4217 register into `OUT_DIR`, from the vendored list.
//!
//! This mirrors `kamu-iso3166`'s build script: the table is generated, never committed, so
//! there is no generated file to keep in step and no way to hand-edit a row. The validation
//! runs here rather than in a test because a test can be skipped and a build failure cannot.

#[path = "build/iso4217.rs"]
mod iso4217;

fn main() {
    // `include_str!` already ties the compilation of THIS script to the file, but a build
    // script's rerun set is otherwise its own declaration — without these, editing the register
    // would not regenerate the table.
    println!("cargo::rerun-if-changed=vendor/list-one.xml");
    println!("cargo::rerun-if-changed=build/iso4217.rs");

    let out =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo")).join("iso4217.rs");
    std::fs::write(&out, iso4217::generate().to_string()).expect("OUT_DIR is writable");
}
