//! Derive one PostgreSQL type per ISO 4217 code from the register.
//!
//! ```text
//! vendor/list-one.xml -> kamu-money-core's register -> this manifest -> macros -> code
//!       (data)                  (enum + markers)        (declensions)     (shape)
//! ```
//!
//! The manifest is **derived**, not maintained beside the register, so it cannot
//! drift from it. There is nothing to verify here because there is no second
//! list that could disagree with the first.
//!
//! # What lives here rather than in the macro
//!
//! Two things a `macro_rules!` cannot do:
//!
//! * **Declension.** It can neither lowercase nor concatenate identifiers, so it
//!   cannot turn `USD` into `kmoney_usd` or `kmoney_usd_out`. Every name a
//!   generated type needs is derived here and passed in already formed.
//! * **DDL.** `extension_sql!` parses its first argument as a string *literal*,
//!   so `CREATE TYPE` text cannot be composed with `concat!` at the macro level.
//!   It has to arrive as a literal, which only a generator can produce.
//!
//! Everything else -- the struct, the sealed impl, the datum ABI, the three I/O
//! functions -- is shape, and shape belongs to `pinned_money_type!`, where one
//! definition governs every currency and none can differ from it.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use kamu_money_core::Iso4217;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let mut shells = String::from("CREATE TYPE kmoney;\nCREATE TYPE kmoney_mixed;\n");
    let mut types = String::new();

    for code in Iso4217::EVERY {
        let alpha3 = code.alpha3();
        let lower = alpha3.to_lowercase();
        let ty = format!("kmoney_{lower}");
        let f_in = format!("{ty}_in");
        let f_out = format!("{ty}_out");
        let f_send = format!("{ty}_send");

        writeln!(shells, "CREATE TYPE {ty};").expect("writing to a String cannot fail");

        write!(
            types,
            r#"
pinned_money_type! {{
    /// {name} ({alpha3}): 16 bytes, canonical units, no currency in the value.
    ///
    /// The column's type is the currency, so `'10.50'::{ty}` needs no tag and a
    /// cross-currency expression has no operator to resolve at all.
    {ty}, kamu_money_core::iso::{alpha3}, {f_in}, {f_out}, {f_send}
}}

extension_sql!(
    r"
CREATE TYPE {ty} (
    INTERNALLENGTH = 16,
    INPUT          = {f_in},
    OUTPUT         = {f_out},
    SEND           = {f_send},
    ALIGNMENT      = char,
    STORAGE        = plain
);
",
    name = "{ty}_concrete",
    requires = [{f_send}, "money_shell_types", {f_in}, {f_out}],
);
"#,
            name = code.name(),
        )
        .expect("writing to a String cannot fail");
    }

    // pgrx permits exactly one `bootstrap` block, and a shell type must exist
    // before the I/O functions that name it. So every shell in the extension is
    // declared here, including the two that are not per-currency.
    let manifest = format!(
        "// @generated from the ISO 4217 register by build.rs. Do not edit.\n\
         extension_sql!(\n    r\"\n{shells}\",\n    name = \"money_shell_types\",\n    bootstrap\n);\n{types}"
    );

    let out =
        PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("pinned_types.rs");
    fs::write(&out, manifest).expect("the manifest must be writable");

    // The count is a contract rather than a coincidence: every code in the
    // register gets a type. A register that changed size must be noticed, not
    // silently generate fewer.
    println!("cargo:rustc-env=KMONEY_PINNED_TYPE_COUNT={}", Iso4217::EVERY.len());
}
