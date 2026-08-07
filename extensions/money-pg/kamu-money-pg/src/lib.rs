//! `kmoney`: money as a native PostgreSQL type.
//!
//! The value is an 18-byte, fixed-width, byte-aligned payload with no varlena header. It carries
//! canonical units and an ISO 4217 numeric code. `numeric(36,18)` is often smaller, but can round
//! excess precision before constraints inspect it.
//!
//! Text parsing, rendering, currency lookup, and arithmetic delegate to `kamu_money_core`.

// Match money-core's named restriction/nursery lints without enabling either
// unstable group wholesale. FFI casts are denied globally; the two required
// typmod reinterpretations carry statement-local exceptions and rationale.
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

use safe::payload::{PAYLOAD_BYTES, PINNED_PAYLOAD_BYTES, Payload, PinnedPayload};

::pgrx::pg_module_magic!(name, version);

mod ffi;
mod safe;

/// Define a **fixed-length** PostgreSQL type over an 18-byte `#[repr(C)]` payload.
///
/// # Why this is not `#[derive(PostgresType)]`
///
/// pgrx 0.19.1's derive hardcodes `INTERNALLENGTH = variable`, so it cannot
/// declare this fixed-width value. The extension therefore owns `CREATE TYPE`
/// and the datum ABI implementations.
///
/// PostgreSQL stores the 18-byte value by reference because `Datum` holds at
/// most 8 bytes. Function results use `palloc` in the current memory context;
/// fixed width removes the varlena header and TOAST eligibility, not allocation.
macro_rules! fixed_length_money_type {
    ($(#[$meta:meta])* $t:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone)]
        #[repr(C)]
        #[allow(non_camel_case_types)]
        pub struct $t(Payload);

        // Compile-time checks bind the Rust layout to `INTERNALLENGTH = 18`
        // and `ALIGNMENT = char`.
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

/// Define a **per-currency** PostgreSQL type over a 16-byte `#[repr(C)]` payload.
///
/// The currency is the SQL type, not a field, so the value is the amount and
/// nothing else. `kmoney_usd + kmoney_idr` therefore has no operator at all and
/// fails while the query is parsed, rather than reaching a currency check
/// inside one.
///
/// # Why the function names are parameters
///
/// A `macro_rules!` can neither lowercase nor concatenate identifiers, so it
/// cannot build `kmoney_usd_out` from `kmoney_usd`. Rather than hand-write the
/// text I/O beside every invocation — 178 chances to mistype a name that
/// nothing would catch — the caller passes every identifier already declined
/// and this macro only *uses* them. Deriving them is `build.rs`'s job, where
/// `format_ident!` can, and the manifest it emits is the flat table of
/// declensions those invocations read from.
///
/// The macro is therefore purely structural: one definition, and every
/// generated currency differs from it in nothing but its names.
macro_rules! pinned_money_type {
    (
        $(#[$meta:meta])*
        $t:ident : $currency:ty,
        concrete = $concrete:literal,
        io = [$in:ident, $out:ident, $send:ident],
        cmp = [$eq:ident, $ne:ident, $lt:ident, $le:ident, $gt:ident, $ge:ident],
        arith = [$add:ident, $sub:ident],
        hash = $hash:ident $(,)?
    ) => {
        $(#[$meta])*
        #[derive(Copy, Clone)]
        #[repr(C)]
        #[allow(non_camel_case_types)]
        pub struct $t(PinnedPayload);

        // Binds the Rust layout to `INTERNALLENGTH = 16` and `ALIGNMENT = char`.
        const _: () = assert!(
            size_of::<$t>() == PINNED_PAYLOAD_BYTES,
            "the struct width must equal the INTERNALLENGTH declared to PostgreSQL"
        );
        const _: () = assert!(
            align_of::<$t>() == 1,
            "ALIGNMENT = char promises PostgreSQL may place this value at any address"
        );

        impl safe::pinned::sealed::Sealed for $t {}

        impl safe::pinned::PinnedCurrency for $t {
            type Currency = $currency;
            // `stringify!` is also what `SqlTranslatable::TYPE_IDENT` uses, so the
            // Rust name and the SQL name agree by construction rather than by
            // assertion -- which is what lets `rust_regtypein` resolve the OID.
            const SQL_NAME: &'static str = stringify!($t);

            fn units(self) -> i128 {
                self.0.units()
            }

            fn from_units(units: i128) -> Self {
                Self(PinnedPayload::from_units(units))
            }
        }

        impl $t {
            /// Canonical units. Inherent as well as on the trait, so the operator
            /// bodies below read as arithmetic rather than as trait dispatch.
            const fn units(self) -> i128 {
                self.0.units()
            }

            const fn payload(self) -> PinnedPayload {
                self.0
            }

            const fn from_payload(payload: PinnedPayload) -> Self {
                Self(payload)
            }
        }

        crate::ffi::impl_pinned_datum!($t);

        #[doc(hidden)]
        #[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
        fn $in(input: &core::ffi::CStr) -> $t {
            match input.to_str() {
                Ok(text) => safe::pinned::parse_pinned(text),
                Err(e) => error!(
                    "{}: input is not valid UTF-8: {e}",
                    <$t as safe::pinned::PinnedCurrency>::SQL_NAME
                ),
            }
        }

        #[doc(hidden)]
        #[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
        fn $out(value: $t) -> alloc::ffi::CString {
            let rendered = safe::pinned::render_pinned(value);
            alloc::ffi::CString::new(rendered).unwrap_or_else(|e| {
                error!(
                    "{}: rendered form contains a NUL byte: {e}",
                    <$t as safe::pinned::PinnedCurrency>::SQL_NAME
                )
            })
        }

        #[doc(hidden)]
        #[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
        fn $send(value: $t) -> Vec<u8> {
            safe::pinned::send_pinned(value)
        }

        // COMPARISON. Within one currency there is nothing left to check: the
        // SQL type already guarantees both operands are this currency, so these
        // read units and nothing else.
        //
        // Note what is absent. `kmoney` calls `same_currency` in every ordering
        // operator and REFUSES a cross-currency comparison at run time. A pinned
        // type has no such call to make and no such refusal to raise: ordering
        // here is TOTAL, because the question it would have refused cannot be
        // asked. Equality is likewise total, and for the same reason.
        //
        // Deliberately NO btree or hash operator class. These are sequential-scan
        // predicates only. The absent default-opclass ordering is the one surface
        // YugabyteDB's planner will not resolve for a custom type, and its
        // absence is what keeps these types byte-exact there.
        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(=)]
        #[commutator(=)]
        #[negator(<>)]
        #[restrict(eqsel)]
        #[join(eqjoinsel)]
        const fn $eq(a: $t, b: $t) -> bool {
            a.units() == b.units()
        }

        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(<>)]
        #[commutator(<>)]
        #[negator(=)]
        #[restrict(neqsel)]
        #[join(neqjoinsel)]
        const fn $ne(a: $t, b: $t) -> bool {
            a.units() != b.units()
        }

        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(<)]
        #[commutator(>)]
        #[negator(>=)]
        #[restrict(scalarltsel)]
        #[join(scalarltjoinsel)]
        const fn $lt(a: $t, b: $t) -> bool {
            a.units() < b.units()
        }

        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(<=)]
        #[commutator(>=)]
        #[negator(>)]
        #[restrict(scalarlesel)]
        #[join(scalarlejoinsel)]
        const fn $le(a: $t, b: $t) -> bool {
            a.units() <= b.units()
        }

        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(>)]
        #[commutator(<)]
        #[negator(<=)]
        #[restrict(scalargtsel)]
        #[join(scalargtjoinsel)]
        const fn $gt(a: $t, b: $t) -> bool {
            a.units() > b.units()
        }

        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(>=)]
        #[commutator(<=)]
        #[negator(<)]
        #[restrict(scalargesel)]
        #[join(scalargejoinsel)]
        const fn $ge(a: $t, b: $t) -> bool {
            a.units() >= b.units()
        }

        // ARITHMETIC. Delegates to the same `kamu-money-core` kernels
        // `Money::checked_add` uses, so the SQL and Rust surfaces cannot
        // disagree about addition.
        //
        // The kernel returns `None` for an out-of-domain operand OR result. A
        // pinned value read from a column can be out of domain -- those bytes
        // came from disk -- so this arm stays. It is the one failure typing
        // cannot remove, because it is a fact about incoming data rather than
        // about this process.
        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(+)]
        #[commutator(+)]
        fn $add(a: $t, b: $t) -> $t {
            let Some(units) =
                kamu_money_core::advanced::arithmetic::add_units(a.units(), b.units())
            else {
                error!(
                    "{}: the result of + is outside the domain |units| <= 10^36 - 1",
                    <$t as safe::pinned::PinnedCurrency>::SQL_NAME
                );
            };
            $t::from_payload(PinnedPayload::from_units(units))
        }

        #[pg_operator(immutable, parallel_safe, requires = [$concrete])]
        #[opname(-)]
        fn $sub(a: $t, b: $t) -> $t {
            let Some(units) =
                kamu_money_core::advanced::arithmetic::sub_units(a.units(), b.units())
            else {
                error!(
                    "{}: the result of - is outside the domain |units| <= 10^36 - 1",
                    <$t as safe::pinned::PinnedCurrency>::SQL_NAME
                );
            };
            $t::from_payload(PinnedPayload::from_units(units))
        }

        /// The stable hash of a pinned value, folded to `int4`.
        ///
        /// Feeds `stable_hash` the currency from the TYPE and the units from the
        /// value, which is the same pair the erased type feeds it from its
        /// payload. So the two hash identically for the same logical amount, and
        /// `STABLE_HASH_VERSION` does not move even though the stored bytes
        /// narrowed from 18 to 16.
        ///
        /// There is no hash operator class here, for the same reason there is no
        /// btree one.
        #[pg_extern(immutable, parallel_safe, requires = [$concrete])]
        fn $hash(value: $t) -> i32 {
            kamu_money_core::advanced::stable_hash::fold_to_i32(
                kamu_money_core::advanced::stable_hash::stable_hash(
                    <<$t as safe::pinned::PinnedCurrency>::Currency
                        as kamu_money_core::StaticCurrency>::CODE
                        .numeric(),
                    value.units(),
                ),
            )
        }
    };
}

// One per-currency type for every ISO 4217 code, derived from the register by
// `build.rs`. It also owns the single `bootstrap` block: pgrx permits only one,
// and every shell type -- including `kmoney` and `kmoney_mixed`, which are not
// per-currency -- must be declared before the I/O functions that name it.
//
// Nothing is decided in the expansion. `build.rs` says what is derived and
// `pinned_money_type!` above says what is generated; between them there is no
// third place for a currency to be described differently.
include!(concat!(env!("OUT_DIR"), "/pinned_types.rs"));

fixed_length_money_type! {
    /// Money, on disk: 18 bytes, fixed width, currency carried in the value.
    ///
    /// The currency lives in the value because PostgreSQL does not pass typmod
    /// to operators: `kmoney(USD) + kmoney(IDR)` arrives as `kmoney + kmoney`.
    ///
    /// # Why this type is `snake_case`
    ///
    /// The Rust name matches the permanent SQL name and lets
    /// `rust_regtypein::<Self>()` resolve its OID without a mapping table.
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
    // Reject unknown or corrupt currency codes instead of rendering a
    // plausible amount under the wrong currency.
    let amount = safe::validated_or_error(value.payload(), "kmoney");
    let rendered = text::render(amount.units(), amount.currency())
        .unwrap_or_else(|e| error!("kmoney: stored amount cannot be rendered: {e}"));
    alloc::ffi::CString::new(rendered)
        .unwrap_or_else(|e| error!("kmoney: rendered form contains a NUL byte: {e}"))
}

fixed_length_money_type! {
    /// Money whose currency is **not** fixed by the column.
    ///
    /// Byte-identical to [`kmoney`] on disk but without arithmetic or ordering
    /// operators. A column may hold several currencies, and
    /// `SELECT sum(amount)` fails while the query is planned:
    ///
    /// ```text
    /// ERROR:  function sum(kmoney_mixed) does not exist
    /// ```
    ///
    /// Convert a value with the SQL function `kmoney_from_mixed`, which checks
    /// the expected currency before returning [`kmoney`].
    kmoney_mixed
}

// No `kmoney -> numeric` cast: egress keeps the exact currency-tagged text form
// instead of exposing PostgreSQL numeric arithmetic.

// Benchmark-only no-ops isolate pgrx wrapper cost (`rs_noop`) and the 18-byte
// `FromDatum` cost (`rs_noop_kmoney`) from function-body work.
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

    // ---------------------------------------------------------------------
    // Per-currency types. These assert the wire claim the shape exists for.
    // ---------------------------------------------------------------------

    /// The column's type is the currency, so egress carries no tag.
    #[pg_test]
    fn a_pinned_type_renders_bare() {
        let rendered =
            Spi::get_one::<String>("SELECT '10.50'::kmoney_usd::text").expect("query ran").expect("not null");
        assert_eq!(
            rendered, "10.50",
            "the currency is the type; repeating it in the text would restate the catalog"
        );
    }

    /// The tag is optional, and checked when present.
    #[pg_test]
    fn a_pinned_type_accepts_its_own_tag() {
        let rendered = Spi::get_one::<String>("SELECT 'USD 10.50'::kmoney_usd::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(rendered, "10.50");
    }

    /// THE wire claim: a well-formed value of another currency is refused, not
    /// reinterpreted. Without this check the digits would simply be accepted.
    #[pg_test(error = "kmoney_usd: expected USD, got IDR")]
    fn a_pinned_type_refuses_another_currencys_tag() {
        Spi::run("SELECT 'IDR 10.50'::kmoney_usd").expect("should have failed");
    }

    /// THE calculation claim: cross-currency arithmetic has no operator to
    /// resolve, so it fails while the query is parsed rather than inside an
    /// operator that had to check.
    #[pg_test(error = "operator does not exist: kmoney_usd + kmoney_idr")]
    fn cross_currency_arithmetic_has_no_operator() {
        Spi::run("SELECT '1.00'::kmoney_usd + '1.00'::kmoney_idr").expect("should have failed");
    }

    /// The currency left the value, so the payload is two bytes narrower than
    /// the erased type still carrying an ISO code.
    #[pg_test]
    fn a_pinned_value_is_sixteen_bytes() {
        let pinned = Spi::get_one::<i32>("SELECT pg_column_size('10.50'::kmoney_usd)")
            .expect("query ran")
            .expect("not null");
        let erased = Spi::get_one::<i32>("SELECT pg_column_size('USD 10.50'::kmoney_mixed)")
            .expect("query ran")
            .expect("not null");
        assert_eq!(pinned, 16, "units only, with no varlena header");
        assert_eq!(erased, 18, "the erased type still stores the ISO code beside them");
    }

    /// Every code in the register got a type, not merely the ones anyone tried.
    #[pg_test]
    fn every_iso_code_has_a_type() {
        let generated = Spi::get_one::<i64>(
            "SELECT count(*) FROM pg_type WHERE typname LIKE 'kmoney\\_%' AND typlen = 16",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(
            usize::try_from(generated).expect("a count fits usize"),
            kamu_money_core::Iso4217::EVERY.len(),
            "the manifest is derived from the register, so the counts cannot disagree"
        );
    }

    /// Ordering is TOTAL within a pinned type.
    ///
    /// `kmoney` calls `same_currency` in every ordering operator and refuses a
    /// cross-currency comparison at run time. Here the operator has nothing to
    /// check, because the question it would refuse cannot be asked.
    #[pg_test]
    fn pinned_ordering_needs_no_currency_check() {
        let ordered = Spi::get_one::<bool>("SELECT '1.00'::kmoney_usd < '2.00'::kmoney_usd")
            .expect("query ran")
            .expect("not null");
        assert!(ordered, "within one currency, ordering is just units");
    }

    /// And the comparison it would have refused has no operator to reach.
    #[pg_test(error = "operator does not exist: kmoney_usd < kmoney_idr")]
    fn cross_currency_ordering_has_no_operator() {
        Spi::run("SELECT '1.00'::kmoney_usd < '1.00'::kmoney_idr").expect("should have failed");
    }

    #[pg_test]
    fn pinned_arithmetic_stays_within_the_currency() {
        let sum = Spi::get_one::<String>("SELECT ('1.25'::kmoney_usd + '2.75'::kmoney_usd)::text")
            .expect("query ran")
            .expect("not null");
        assert_eq!(sum, "4.00");
    }

    /// A pinned value hashes exactly as the erased one does for the same logical
    /// amount.
    ///
    /// This is why `STABLE_HASH_VERSION` did not move when the payload narrowed
    /// from 18 bytes to 16: both feed `stable_hash` the same `(code, units)`
    /// pair, the pinned type from its *type* and the erased one from its
    /// *payload*. The storage width changed; the hashed value did not.
    #[pg_test]
    fn a_pinned_value_hashes_as_the_erased_one_does() {
        let pinned = Spi::get_one::<i32>("SELECT kmoney_usd_hash('10.50'::kmoney_usd)")
            .expect("query ran")
            .expect("not null");
        let erased = Spi::get_one::<i32>("SELECT kmoney_mixed_hash('USD 10.50'::kmoney_mixed)")
            .expect("query ran")
            .expect("not null");
        assert_eq!(pinned, erased, "same amount, same stable hash, whatever the storage width");
    }

    /// No generated type carries a btree or hash operator class.
    ///
    /// Their absence is not an omission. It is the one surface YugabyteDB's
    /// planner will not resolve for a custom type, and removing it is what makes
    /// these types byte-exact there. An opclass added later would break that for
    /// every currency at once, so the assertion covers all of them.
    #[pg_test]
    fn no_generated_type_carries_an_operator_class() {
        let classes = Spi::get_one::<i64>(
            "SELECT count(*) FROM pg_opclass WHERE opcintype IN \
             (SELECT oid FROM pg_type WHERE typname LIKE 'kmoney\\\\_%' AND typlen = 16)",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(classes, 0, "an operator class here would cost YugabyteDB byte-exactness");
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
