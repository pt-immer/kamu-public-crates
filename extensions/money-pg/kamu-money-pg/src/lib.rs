//! `kmoney`: money as a native PostgreSQL type.
//!
//! The value is an 18-byte, fixed-width, byte-aligned payload with no varlena header. It carries
//! canonical units and an ISO 4217 numeric code. `numeric(36,18)` is often smaller, but can round
//! excess precision before constraints inspect it.
//!
//! Text parsing, rendering, currency lookup, and arithmetic delegate to `kamu_money_core`.

// kamu-money-core's lint posture, mirrored here. Both crates cherry-pick from `clippy::restriction`
// and `clippy::nursery` BY NAME rather than enabling either group, for the reason kamu-money-core
// records: `restriction` is self-contradictory by design and `nursery` is under development, so
// denying a whole group lets a toolchain upgrade break the build for reasons unrelated to this
// code.
//
// This is the C ABI crate, so casts are load-bearing rather than incidental — which is exactly
// why the lints are ON. Every `as` that survives is a deliberate reinterpret, and each one takes
// its exception NARROWLY, on the statement where it fires, with the reason written beside it:
//   - `typmod as i32` in kmoney_typmod_out (sign-extends PostgreSQL's -1 sentinel; `try_from`
//     would reject it, so the lint's "safe" fix would be the bug).
//   - varlena -> ArrayType in kmoney_typmod_in (palloc is MAXALIGN'd, so the pointer is
//     over-aligned; the same cast PostgreSQL's own DatumGetArrayTypeP performs).
// A function-wide allow was rejected: it would blanket-permit future unaudited casts in the very
// functions that most need auditing.
#![deny(clippy::all, clippy::pedantic)]
#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]
#![deny(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_ptr_alignment,
    clippy::cast_sign_loss,
    clippy::missing_const_for_fn,
    clippy::use_self
)]

use kamu_money_core::text;

extern crate alloc;
use pgrx::prelude::*;

use safe::payload::{PAYLOAD_BYTES, Payload};

::pgrx::pg_module_magic!(name, version);

mod ffi;
mod safe;

/// Define a **fixed-length** PostgreSQL type over an 18-byte `#[repr(C)]` payload.
///
/// # Why this is not `#[derive(PostgresType)]`
///
/// pgrx's derive emits `INTERNALLENGTH = variable` as a hardcoded string literal
/// (`pgrx-sql-entity-graph-0.19.1`, `postgres_type/entity.rs`) — it is not a parameter, so every
/// derived type is a varlena. That costs a header on every value: 1 byte on disk, 4 in memory,
/// plus TOAST eligibility that an 18-byte payload will never use.
///
/// PostgreSQL does not require that. `uuid` is `typlen = 16, typbyval = f, typalign = c,
/// typstorage = p` — a fixed-length, 1-byte-aligned, plain, non-varlena type, which is exactly
/// this shape at 16 instead of 18. Reaching it means owning the datum path: the `CREATE TYPE` is
/// hand-written below, and so are `IntoDatum`, `FromDatum`, `SqlTranslatable`, `ArgAbi` and
/// `BoxRet`.
///
/// **What this does NOT buy**: pass-by-value. `typbyval` requires `typlen <= 8` because a
/// `Datum` is 8 bytes, and 128 bits of units plus a currency is 18. `uuid`, `interval` and
/// `point` are all pass-by-reference for the same reason, so a `palloc` per function result is
/// inherent to any wide PostgreSQL type rather than a cost this design chose. It is a bump
/// allocation in a per-tuple memory context that is reset wholesale, not a `malloc`.
macro_rules! fixed_length_money_type {
    ($(#[$meta:meta])* $t:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone)]
        #[repr(C)]
        #[allow(non_camel_case_types)]
        pub struct $t(Payload);

        // Held where they cannot rot. `size_of` is the on-disk width PostgreSQL is told about
        // in `INTERNALLENGTH`, and a mismatch would read past the end of a tuple field;
        // `align_of` must be 1 because `ALIGNMENT = char` promises PostgreSQL may place the
        // value anywhere. Both are compile errors, not tests.
        const _: () = assert!(
            size_of::<$t>() == PAYLOAD_BYTES,
            "the struct width must equal the INTERNALLENGTH declared to PostgreSQL"
        );
        const _: () = assert!(
            align_of::<$t>() == 1,
            "ALIGNMENT = char promises PostgreSQL may place this value at any address"
        );

        impl $t {
            /// Canonical units, as `kamu_money_core` counts them.
            fn units(self) -> i128 {
                self.0.units()
            }

            /// The stored ISO 4217 numeric code.
            const fn code(self) -> u16 {
                self.0.code()
            }

            fn new(units: i128, code: u16) -> Self {
                Self(Payload::from_parts(units, code))
            }

            const fn payload(self) -> Payload {
                self.0
            }

            const fn from_payload(payload: Payload) -> Self {
                Self(payload)
            }
        }

    };
}

// The shell type must exist before the in/out functions can name it, and the real type before
// anything else can use it. `bootstrap` puts this first in the generated script.
// Both shell types in one block: pgrx permits exactly one `bootstrap` positioning, and a
// shell has to exist before an in/out function can name the type in its signature.
extension_sql!(
    r"
CREATE TYPE kmoney;
CREATE TYPE kmoney_mixed;
",
    name = "money_shell_types",
    bootstrap
);

fixed_length_money_type! {
    /// Money, on disk: 18 bytes, fixed width, currency carried in the value.
    ///
    /// The currency lives in the value because PostgreSQL does not pass typmod to operators:
    /// `kmoney(USD) +
    /// kmoney(IDR)` reaches the operator as `kmoney + kmoney` and the only thing that can
    /// tell them apart is the value itself.
    ///
    /// # Why this type is `snake_case`
    ///
    /// The SQL name is the permanent public interface, so the Rust name matches `kmoney`.
    /// Nothing imports this crate
    /// as a Rust library, it is a `cdylib`. Keeping the two identical also means
    /// `rust_regtypein::<Self>()`, which resolves the OID from the Rust type's last path
    /// segment, finds the right type without a mapping table.
    kmoney
}

#[doc(hidden)]
#[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
fn kmoney_in(input: &core::ffi::CStr) -> kmoney {
    let text = match input.to_str() {
        Ok(t) => t,
        Err(e) => error!("kmoney: input is not valid UTF-8: {e}"),
    };
    // The type input function refuses excess precision before PostgreSQL coercion can round it.
    match text::parse(text) {
        Ok((currency, units)) => kmoney::new(units, currency.numeric()),
        Err(e) => error!("kmoney: {e}, in {text:?}"),
    }
}

#[doc(hidden)]
#[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
fn kmoney_out(value: kmoney) -> alloc::ffi::CString {
    // An unknown code means the row was written by a build whose currency table differed, or
    // the bytes are corrupt. Rendering a placeholder would emit a number attached to the wrong
    // currency, which is precisely the silent wrongness this design exists to prevent.
    let amount = safe::validated_or_error(value.payload(), "kmoney");
    // A stored amount outside the domain means corrupt bytes or a datum written by something
    // that bypassed the input function. `text::render` refuses it rather than emitting
    // canonical-looking text no parser would accept back, so this surfaces as a SQL ERROR on
    // the row that is actually broken.
    let rendered = text::render(amount.units(), amount.currency())
        .unwrap_or_else(|e| error!("kmoney: stored amount cannot be rendered: {e}"));
    alloc::ffi::CString::new(rendered)
        .unwrap_or_else(|e| error!("kmoney: rendered form contains a NUL byte: {e}"))
}

// =========================================================================================
// TYPMOD: `kmoney(IDR)` pins a column to one currency.
//
// These four functions are RAW `extern "C"` because pgrx 0.19.1 cannot express them. PostgreSQL
// hands `typmod_in` a `cstring[]`, and pgrx has no safe mapping for that -- a
// `#[pg_extern] fn(Vec<Option<&CStr>>)` fails to compile with "cannot be passed into a Postgres
// function as a Datum", and the crate's only typmod support is const-generic and built-in-only
// (`Numeric<P, S>`). So the array is parsed against `pg_sys` directly and the SQL is declared by
// hand.
//
// The typmod VALUE is the ISO 4217 numeric code. PostgreSQL reserves -1 for "no modifier" and
// every assigned code is 1..=999, so the two never collide and no encoding is needed.
//
// This does not make cross-currency arithmetic fail: PostgreSQL does
// not pass typmod to operators, so `kmoney(USD) + kmoney(IDR)` still arrives as
// `kmoney + kmoney`. Typmod is a column-level INSERT/coercion check and nothing more; the
// value-carried code remains the only thing standing between two currencies in an expression.
// =========================================================================================

// =========================================================================================
// Arithmetic — defined for `kmoney` and, deliberately, for NOTHING ELSE.
//
// `kmoney_mixed` below has no `+`, no `-`, and no sum of any kind. That
// is stronger than a runtime check, because `SELECT sum(amount)` on a mixed column fails at
// PLAN time — before a row is read — rather than on row 4,000,000 of a nightly batch. It is
// the SQL analogue of `Add` existing only on `Money<C>`: the unproven form cannot be added
// because the impl is not there.
//
// `kmoney` has `+`, `-`, variadic `kmoney_sum`, and a `sum` aggregate with a wide transition
// state. Partial totals therefore cannot fail solely because of row or combine order.
// =========================================================================================

fixed_length_money_type! {
    /// Money whose currency is **not** fixed by the column.
    ///
    /// Byte-identical to [`kmoney`] on disk, and that is the point: the difference is not the
    /// representation, it is the **absence of an operator surface**. A column declared
    /// `kmoney_mixed` may hold rows in different currencies, and `SELECT sum(amount)` over it
    /// fails when the query is planned:
    ///
    /// ```text
    /// ERROR:  function sum(kmoney_mixed) does not exist
    /// ```
    ///
    /// A runtime check would have read four million rows first and then failed on the one that
    /// disagreed. This fails before the scan, on every such query, deterministically, whether
    /// or not the data happens to be homogeneous today.
    ///
    /// To compute with these values, prove a row into `kmoney` with `kmoney_from_mixed` (the SQL function; the Rust item behind it is private) —
    /// exactly as Rust proves a value into a typed `Money<C>` before it can be added.
    kmoney_mixed
}

// There is no `kmoney -> numeric` cast. Such a cast would expose PostgreSQL's unconstrained
// numeric operators and their value-dependent rounding. Egress uses the exact, currency-tagged
// text form instead. Tests may still compare storage and ingress behavior with `numeric`.

// =========================================================================================
// THE BOUNDARY PROBE --- what a pgrx call costs, and nothing else.
//
// Behind `--features boundary-probe`, so none of this is in the shipped SQL surface. Adding a
// no-op to the extension to support a benchmark is a trade this workspace has not made; adding
// one that is not compiled unless a benchmark asks for it is a different trade.
//
// `c_noop` in
// `kamu-money-pg/bench/boundary/c_noop.c` is `bigint -> bigint` returning its argument, and so
// is `rs_noop`. Everything the pgrx one costs above the C one is the wrapper: fmgr dispatch is
// common to both. `rs_noop_kmoney` then adds exactly one thing --- `FromDatum` for the 18-byte
// type --- so the type's conversion separates from any function body.
//
// DO NOT give these bodies. A body is the thing being subtracted out.
#[cfg(feature = "boundary-probe")]
#[pg_extern(immutable, parallel_safe)]
fn rs_noop(x: i64) -> i64 {
    x
}

#[cfg(feature = "boundary-probe")]
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn rs_noop_kmoney(m: kmoney) -> i64 {
    // Returns a constant, not a property of `m`: the cost being measured is getting `m` across
    // the boundary at all. Reading a field would add a load to one side of the comparison.
    let _ = m;
    0
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// The payload is 18 bytes both in a tuple and as an expression, proving there is no
    /// varlena header.
    #[pg_test]
    fn kmoney_is_eighteen_bytes_with_no_header() {
        Spi::run("CREATE TABLE sized (v kmoney)").expect("table created");
        Spi::run("INSERT INTO sized VALUES ('USD 10.50')").expect("row inserted");

        let stored =
            Spi::get_one::<i32>("SELECT pg_column_size(v) FROM sized").expect("query ran").expect("not null");
        let in_memory = Spi::get_one::<i32>("SELECT pg_column_size('USD 10.50'::kmoney)")
            .expect("query ran")
            .expect("not null");

        assert_eq!(stored, 18, "the payload, with no varlena header");
        assert_eq!(
            in_memory, stored,
            "a fixed-length type has no in-memory header either; a varlena would read 22 here"
        );
    }

    /// PostgreSQL agrees it is fixed-length — the catalog, not just `pg_column_size`.
    ///
    /// `typlen = 18` (not `-1`), `typbyval = false` (18 > 8, so pass-by-reference is forced),
    /// `typalign = 'c'` and `typstorage = 'p'`. Exactly `uuid`'s shape, two bytes wider.
    #[pg_test]
    fn the_catalog_says_fixed_length_plain_and_byte_aligned() {
        let row = Spi::get_one::<String>(
            "SELECT string_agg(
                 format('%s=%s/%s/%s/%s', typname, typlen, typbyval, typalign, typstorage),
                 ',' ORDER BY typname
             )
               FROM pg_type WHERE typname IN ('kmoney', 'kmoney_mixed')",
        )
        .expect("query ran")
        .expect("not null");
        // PostgreSQL renders booleans as t/f, so typbyval = false prints as "f".
        assert_eq!(row, "kmoney=18/f/c/p,kmoney_mixed=18/f/c/p");
    }

    /// **`kmoney` is NOT smaller than `numeric(36,18)` for typical amounts.** Measured:
    ///
    /// | stored value          | `kmoney` | `numeric(36,18)` |
    /// |-----------------------|-----------:|-----------------:|
    /// | `0`                   |         18 |                3 |
    /// | `0.000000000000000001`|         18 |                5 |
    /// | `10.50`               |         18 |                7 |
    /// | domain top            |         18 |               23 |
    ///
    /// `numeric` is variable-width and drops trailing zeros, so it beats a fixed 18 bytes
    /// everywhere except the top of the domain — and even the top comparison flatters
    /// `kmoney`, since a real schema needs a currency column beside the `numeric` and this
    /// type carries its own in those 18 bytes.
    ///
    /// The type optimizes representation stability and exact ingress, not typical-value size.
    #[pg_test]
    fn the_size_tradeoff_against_numeric_is_measured_not_assumed() {
        Spi::run("CREATE TABLE compared (r kmoney, n numeric(36,18))").expect("table created");
        Spi::run(
            "INSERT INTO compared VALUES
                 ('USD 10.50', 10.50),
                 ('IDR 999999999999999999.999999999999999999',
                  999999999999999999.999999999999999999)",
        )
        .expect("rows inserted");

        let (typical_r, typical_n) = Spi::get_two::<i32, i32>(
            "SELECT pg_column_size(r), pg_column_size(n) FROM compared ORDER BY n LIMIT 1",
        )
        .expect("query ran");
        let (top_r, top_n) = Spi::get_two::<i32, i32>(
            "SELECT pg_column_size(r), pg_column_size(n) FROM compared ORDER BY n DESC LIMIT 1",
        )
        .expect("query ran");

        let (typical_r, typical_n) = (typical_r.expect("not null"), typical_n.expect("not null"));
        let (top_r, top_n) = (top_r.expect("not null"), top_n.expect("not null"));

        assert_eq!(typical_r, 18, "fixed width, whatever the value");
        assert_eq!(top_r, 18, "fixed width, whatever the value");

        assert!(
            typical_n < typical_r,
            "numeric is expected to WIN on a typical amount: numeric {typical_n} vs kmoney \
             {typical_r}. If this reverses, remeasure and update the documented tradeoff."
        );
        assert!(
            top_n > top_r,
            "at the domain top numeric should exceed the fixed width: numeric {top_n} vs \
             kmoney {top_r}"
        );
    }

    /// Size must not vary with the value. That is what "fixed-size, no limb codec" means, and
    /// it is the entire economic argument for a native type over `numeric`.
    #[pg_test]
    fn the_size_does_not_depend_on_the_value() {
        Spi::run("CREATE TABLE varied (v kmoney)").expect("table created");
        Spi::run(
            "INSERT INTO varied VALUES
                 ('USD 0.00'),
                 ('USD 10.50'),
                 ('IDR 999999999999999999.999999999999999999'),
                 ('JPY -1')",
        )
        .expect("rows inserted");

        let distinct = Spi::get_one::<i64>("SELECT count(DISTINCT pg_column_size(v)) FROM varied")
            .expect("query ran")
            .expect("not null");
        assert_eq!(distinct, 1, "every value must occupy the same space");
    }

    /// The text form round-trips through the database unchanged and matches
    /// `kamu_money_core`'s canonical rendering.
    #[pg_test]
    fn the_text_form_matches_money_core() {
        for (input, expected) in [
            ("USD 10.50", "USD 10.50"),
            ("USD 10.5", "USD 10.50"),  // liberal in, canonical out
            ("JPY 10.5", "JPY 10.5"),   // settles at 0dp
            ("KWD 10.5", "KWD 10.500"), // settles at 3dp
            ("USD -0.000000000000000001", "USD -0.000000000000000001"),
        ] {
            let got = Spi::get_one::<String>(&format!("SELECT '{input}'::kmoney::text"))
                .expect("query ran")
                .expect("not null");
            assert_eq!(got, expected, "input {input}");
        }
    }

    /// PostgreSQL `numeric(36,18)` rounds this over-precise value to zero.
    ///
    /// The refusal half is [`kmoney_refuses_what_numeric_silently_rounds`] below. They are two
    /// tests because a raised PostgreSQL `ERROR` longjmps out of `Spi`, aborting the
    /// transaction — so a single test cannot both observe a success and catch a failure, and
    /// `.is_err()` on `Spi::get_one` never gets the chance to be false.
    #[pg_test]
    fn numeric_silently_rounds_four_e_minus_nineteen_to_zero() {
        let rounded = Spi::get_one::<bool>("SELECT '0.0000000000000000004'::numeric(36,18) = 0")
            .expect("query ran")
            .expect("not null");
        assert!(rounded, "numeric(36,18) rounds 4e-19 to zero");
    }

    /// `kmoney` refuses exactly what `numeric(36,18)` swallows.
    ///
    /// This is the difference an input function makes: it runs BEFORE any coercion, where a
    /// `CHECK` or `DOMAIN` runs after and is handed the already-altered value.
    // The expected message is one long line on purpose: pgrx parses this attribute by taking
    // the literal's source text and running it through an `unescape` pass, so a `\`-newline
    // continuation is not reliably folded away and would be compared verbatim.
    #[pg_test(
        error = "kmoney: 19 fractional digits exceeds the supported scale of 18, in \"USD 0.0000000000000000004\""
    )]
    fn kmoney_refuses_what_numeric_silently_rounds() {
        Spi::get_one::<String>("SELECT 'USD 0.0000000000000000004'::kmoney::text").ok();
    }

    /// The top of the domain is representable — the bound is `<=`, not `<`.
    #[pg_test]
    fn the_domain_top_round_trips() {
        let top = Spi::get_one::<String>("SELECT 'IDR 999999999999999999.999999999999999999'::kmoney::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(top, "IDR 999999999999999999.999999999999999999");
    }

    /// One major unit past the domain is refused by the same check `kamu_money_core` applies.
    #[pg_test(
        error = "kmoney: 1000000000000000000000000000000000000 canonical units is outside the supported range -999999999999999999999999999999999999..=999999999999999999999999999999999999, in \"IDR 1000000000000000000\""
    )]
    fn one_unit_past_the_domain_is_refused() {
        Spi::get_one::<String>("SELECT 'IDR 1000000000000000000'::kmoney::text").ok();
    }

    /// A currency `kamu_money_core` does not know is refused at input, not stored and guessed at
    /// later. There is exactly one currency table, in `kamu_money_core`.
    #[pg_test(error = "kmoney: invalid money literal, in \"ZWL 1.00\"")]
    fn an_unknown_currency_is_refused_at_the_boundary() {
        Spi::get_one::<String>("SELECT 'ZWL 1.00'::kmoney::text").ok();
    }

    /// Native and text storage must expose the same canonical representation.
    ///
    /// Both columns are written from the SAME literal here, and the assertion is that
    /// `kmoney`'s output function reproduces the text form exactly. Divergence would make the
    /// portable and native columns return different representations for the same amount.
    #[pg_test]
    fn the_native_type_and_the_text_storage_agree() {
        Spi::run(
            "CREATE TABLE both_forms (
                 portable text  NOT NULL,
                 native kmoney  NOT NULL
             )",
        )
        .expect("table created");

        // One literal, both columns. The text column takes it verbatim; the kmoney column
        // parses it through this extension's input function.
        for literal in [
            "USD 10.50",
            "JPY 10.5",
            "KWD 10.500",
            "IDR 999999999999999999.999999999999999999",
            "USD -0.000000000000000001",
            "XAU 10.5",
        ] {
            Spi::run(&format!("INSERT INTO both_forms VALUES ('{literal}', '{literal}')"))
                .unwrap_or_else(|e| panic!("insert {literal}: {e}"));
        }

        // Rendering the native column back must reproduce the stored text, for every row.
        let disagreements =
            Spi::get_one::<i64>("SELECT count(*) FROM both_forms WHERE portable <> native::text")
                .expect("query ran")
                .expect("not null");
        assert_eq!(disagreements, 0, "the text storage and the native type must render identically");

        // And the reverse direction: text parsed into the native type equals the native value.
        let mismatches = Spi::get_one::<i64>(
            "SELECT count(*) FROM both_forms WHERE portable::kmoney::text <> native::text",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(mismatches, 0, "text -> kmoney -> text must be the identity");
    }

    #[pg_test(error = "cannot cast type kmoney to numeric")]
    fn there_is_no_cast_to_numeric() {
        Spi::get_one::<String>("SELECT ('USD 1.00'::kmoney)::numeric::text").ok();
    }
}

/// Required by `cargo pgrx test`.
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    #[must_use]
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
