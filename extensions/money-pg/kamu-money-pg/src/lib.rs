//! `kmoney` — money as a native PostgreSQL type. (specs.md C8)
//!
//! The storage claim this crate makes good on: a **fixed-size payload, no varlena decode, no
//! limb codec**, so reading a value is a header read and a cast.
//!
//! **18 bytes on disk and in memory**, fixed width, no varlena header — the same shape `uuid`
//! has (`typlen = 16, typbyval = f, typalign = c, typstorage = p`), two bytes wider.
//!
//! That number took three measurements to settle, and every one of them corrected a claim this
//! document had already made. E12 predicted 19 by reading pgrx's source; the first
//! `pg_column_size` returned **36**, because `i128`'s 16-byte alignment padded the struct to 32
//! and — worse — made the reference pgrx hands out unsound. Byte-array fields brought it to the
//! predicted 19, but only on disk: as an expression it was 22, since a varlena carries a 4-byte
//! header in memory. Leaving varlena behind removed the header from both. See E14.
//!
//! It is **not** a space win over `numeric(36,18)`, which is variable-width and smaller for
//! every amount short of the domain top. What it buys is a value that cannot be stored without
//! its currency, a width that does not move with the data, and a refusal of the precision
//! `numeric` silently rounds away (E13).
//!
//! Every digit of the text form and every currency lookup comes from `kamu_money_core::text`. C9
//! requires the adapters to be thin over one codec, and this crate holds no table of its own:
//! a currency that `kamu_money_core` does not know is an error here rather than a second opinion.

// kamu-money-core's lint posture, mirrored here. Both crates cherry-pick from `clippy::restriction`
// and `clippy::nursery` BY NAME rather than enabling either group, for the reason kamu-money-core
// records: `restriction` is self-contradictory by design and `nursery` is under development, so
// denying a whole group lets a toolchain upgrade break the build for reasons unrelated to this
// code. (specs.md C10)
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

use kamu_money_core::Iso4217;
use kamu_money_core::text;

extern crate alloc;
use pgrx::datum::{FromDatum, IntoDatum};
use pgrx::prelude::*;

::pgrx::pg_module_magic!(name, version);

/// The payload every money type here stores: 16 bytes of little-endian units followed by a
/// 2-byte little-endian ISO 4217 numeric code.
const PAYLOAD_BYTES: usize = 18;

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
        pub struct $t {
            units: [u8; 16],
            code: [u8; 2],
        }

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
            const fn units(self) -> i128 {
                i128::from_le_bytes(self.units)
            }

            /// The stored ISO 4217 numeric code.
            const fn code(self) -> u16 {
                u16::from_le_bytes(self.code)
            }

            const fn new(units: i128, code: u16) -> Self {
                Self { units: units.to_le_bytes(), code: code.to_le_bytes() }
            }

            /// The on-disk bytes. Assembled field by field rather than by transmuting the
            /// struct, so the format is stated here rather than inherited from `repr(C)`.
            fn to_payload(self) -> [u8; PAYLOAD_BYTES] {
                let mut out = [0u8; PAYLOAD_BYTES];
                out[..16].copy_from_slice(&self.units);
                out[16..].copy_from_slice(&self.code);
                out
            }

            /// Takes an ARRAY, not a slice: `copy_from_slice` panics on a length mismatch, and
            /// a `&[u8; PAYLOAD_BYTES]` moves that check to the type where it cannot fire at
            /// run time at all.
            fn from_payload(bytes: &[u8; PAYLOAD_BYTES]) -> Self {
                let (units, code) = bytes.split_at(16);
                Self {
                    units: units.try_into().expect("split_at(16) yields 16 bytes"),
                    code: code.try_into().expect("PAYLOAD_BYTES - 16 == 2"),
                }
            }
        }

        impl IntoDatum for $t {
            fn into_datum(self) -> Option<pgrx::pg_sys::Datum> {
                let payload = self.to_payload();
                // `palloc` + `copy_nonoverlapping`, NOT `palloc_slice`. palloc_slice builds a
                // `&mut [u8]` over memory palloc has not initialised, and constructing a
                // reference to uninitialised bytes is undefined behaviour even for `u8` --
                // benign here because the next line overwrites all of it, but it is UB the
                // compiler is entitled to act on, and Miri rejects it. A raw pointer never
                // makes that claim.
                //
                // SAFETY: CurrentMemoryContext is always valid inside a backend; palloc returns
                // at least PAYLOAD_BYTES of writable memory or raises; source and destination
                // are distinct allocations of exactly that length. PostgreSQL owns the result
                // and frees it on context reset -- this must NOT be freed here.
                let dst = unsafe {
                    pgrx::pg_sys::palloc(PAYLOAD_BYTES).cast::<u8>()
                };
                unsafe {
                    core::ptr::copy_nonoverlapping(payload.as_ptr(), dst, PAYLOAD_BYTES);
                }
                Some(dst.into())
            }

            fn type_oid() -> pgrx::pg_sys::Oid {
                pgrx::wrappers::rust_regtypein::<Self>()
            }
        }

        impl FromDatum for $t {
            unsafe fn from_polymorphic_datum(
                datum: pgrx::pg_sys::Datum,
                is_null: bool,
                _typoid: pgrx::pg_sys::Oid,
            ) -> Option<Self> {
                if is_null {
                    return None;
                }
                // SAFETY: a non-null datum of this type is a pointer to PAYLOAD_BYTES that
                // PostgreSQL laid out, and `align_of == 1` (asserted above) means any address
                // it chose is readable. The bytes are COPIED out rather than borrowed, so the
                // result does not alias the tuple.
                let bytes = unsafe {
                    &*datum.cast_mut_ptr::<u8>().cast::<[u8; PAYLOAD_BYTES]>()
                };
                Some(Self::from_payload(bytes))
            }
        }

        // Lets this type be an ARRAY element, which is what a `VARIADIC $t[]` argument is. pgrx
        // gates array iteration on `UnboxDatum` rather than `FromDatum`; the two read the same
        // inline PAYLOAD_BYTES, but this one has no `is_null` branch because the array iterator
        // has already decided nullness. Mirrors the `Uuid` impl (a 16-byte by-ref type) exactly,
        // two bytes wider. Without it `kmoney_sum(VARIADIC kmoney[])` will not compile.
        unsafe impl pgrx::datum::UnboxDatum for $t {
            type As<'src> = $t;
            unsafe fn unbox<'src>(datum: pgrx::datum::Datum<'src>) -> Self::As<'src>
            where
                Self: 'src,
            {
                // SAFETY: the array stores each non-null element inline as PAYLOAD_BYTES that
                // PostgreSQL laid out, and `align_of == 1` means the address it chose is
                // readable. Read by value (`$t` is `Copy`) so the result does not alias the array.
                let bytes = unsafe {
                    datum
                        .sans_lifetime()
                        .cast_mut_ptr::<[u8; PAYLOAD_BYTES]>()
                        .read()
                };
                Self::from_payload(&bytes)
            }
        }

        unsafe impl pgrx::pgrx_sql_entity_graph::metadata::SqlTranslatable for $t {
            const TYPE_IDENT: &'static str = stringify!($t);
            const TYPE_ORIGIN: pgrx::pgrx_sql_entity_graph::metadata::TypeOrigin =
                pgrx::pgrx_sql_entity_graph::metadata::TypeOrigin::External;
            const ARGUMENT_SQL: Result<
                pgrx::pgrx_sql_entity_graph::metadata::SqlMappingRef,
                pgrx::pgrx_sql_entity_graph::metadata::ArgumentError,
            > = Ok(pgrx::pgrx_sql_entity_graph::metadata::SqlMappingRef::literal(stringify!($t)));
            const RETURN_SQL: Result<
                pgrx::pgrx_sql_entity_graph::metadata::ReturnsRef,
                pgrx::pgrx_sql_entity_graph::metadata::ReturnsError,
            > = Ok(pgrx::pgrx_sql_entity_graph::metadata::ReturnsRef::One(
                pgrx::pgrx_sql_entity_graph::metadata::SqlMappingRef::literal(stringify!($t)),
            ));
        }

        unsafe impl<'fcx> pgrx::callconv::ArgAbi<'fcx> for $t {
            unsafe fn unbox_arg_unchecked(arg: pgrx::callconv::Arg<'_, 'fcx>) -> Self {
                let index = arg.index();
                unsafe {
                    arg.unbox_arg_using_from_datum()
                        .unwrap_or_else(|| panic!("argument {index} must not be null"))
                }
            }

            /// pgrx's trait docs are explicit that a BY-REFERENCE type must override this,
            /// because Postgres conflates "SQL null" with "nullptr" in places. `typbyval = f`
            /// makes this exactly that case, and pgrx's own `argue_from_datum!` -- which the
            /// impl above otherwise copies verbatim -- supplies both halves.
            ///
            /// Not reachable today: no `#[pg_extern]` here takes an `Option<kmoney>`, so every
            /// argument routes through `unbox_arg_unchecked`. It becomes live the moment one
            /// does, which is precisely the kind of gap that is cheap now and a debugging
            /// session later.
            unsafe fn unbox_nullable_arg(
                arg: pgrx::callconv::Arg<'_, 'fcx>,
            ) -> pgrx::nullable::Nullable<Self> {
                unsafe { arg.unbox_arg_using_from_datum() }.into()
            }
        }

        unsafe impl pgrx::callconv::BoxRet for $t {
            unsafe fn box_into<'fcx>(
                self,
                fcinfo: &mut pgrx::callconv::FcInfo<'fcx>,
            ) -> pgrx::datum::Datum<'fcx> {
                match self.into_datum() {
                    Some(datum) => unsafe { fcinfo.return_raw_datum(datum) },
                    None => fcinfo.return_null(),
                }
            }
        }
    };
}

mod wire;

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
    /// The currency lives in the **value**, not the type, and C8 measured why that is forced
    /// rather than chosen: PostgreSQL does not pass typmod to operators, so `kmoney(USD) +
    /// kmoney(IDR)` reaches the operator as `kmoney + kmoney` and the only thing that can
    /// tell them apart is the value itself.
    ///
    /// # Why this type is `snake_case`
    ///
    /// The SQL name is the permanent public interface of a database extension and specs.md C8
    /// fixes it as `kmoney`, so the Rust name is what gives way — nothing imports this crate
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
    // **Refuses excess precision rather than rounding.** E13 measured PostgreSQL's own
    // numeric(36,18) cast silently storing '0.0000000000000000004' as ZERO, with no error, and
    // no CHECK or DOMAIN able to catch it because constraints run AFTER the cast. A type input
    // function runs BEFORE any coercion, which is exactly why kmoney can refuse where
    // numeric cannot.
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
    let currency = currency_or_error(value.code(), "kmoney");
    // A stored amount outside the domain means corrupt bytes or a datum written by something
    // that bypassed the input function. `text::render` refuses it rather than emitting
    // canonical-looking text no parser would accept back, so this surfaces as a SQL ERROR on
    // the row that is actually broken.
    let rendered = text::render(value.units(), currency)
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
// **This does not make cross-currency arithmetic fail**, and C8 measured why: PostgreSQL does
// not pass typmod to operators, so `kmoney(USD) + kmoney(IDR)` still arrives as
// `kmoney + kmoney`. Typmod is a column-level INSERT/coercion check and nothing more; the
// value-carried code remains the only thing standing between two currencies in an expression.
// =========================================================================================

/// PostgreSQL's V1 calling-convention record, one per raw function.
///
/// `#[pg_extern]` emits this for you; a hand-declared `LANGUAGE c` function has to supply it or
/// PostgreSQL refuses the call with
/// `could not find function information for function "..."`. The name must be exactly
/// `pg_finfo_<symbol>`.
// `extern "C"`, not `"C-unwind"`: this returns a pointer to a static and cannot panic, so
// there is nothing to unwind. `#[pg_guard]`'s C-unwind requirement applies to the functions it
// wraps, not to the finfo record beside them.
macro_rules! pg_finfo_v1 {
    ($name:ident, $finfo:ident) => {
        static $name: pg_sys::Pg_finfo_record = pg_sys::Pg_finfo_record { api_version: 1 };

        #[unsafe(no_mangle)]
        pub extern "C" fn $finfo() -> *const pg_sys::Pg_finfo_record {
            &raw const $name
        }
    };
}

pg_finfo_v1!(FINFO_TYPMOD_IN, pg_finfo_kmoney_typmod_in);
pg_finfo_v1!(FINFO_TYPMOD_OUT, pg_finfo_kmoney_typmod_out);
// The binary-input functions need the same records. Declared HERE, beside the others,
// because `macro_rules!` is textually scoped -- invoking `pg_finfo_v1!` above its own
// definition fails with `cannot find macro in this scope`, which is what happened.
pg_finfo_v1!(FINFO_RECV, pg_finfo_kmoney_recv);
pg_finfo_v1!(FINFO_MIXED_RECV, pg_finfo_kmoney_mixed_recv);

mod typmod;

// =========================================================================================
// Arithmetic — defined for `kmoney` and, deliberately, for NOTHING ELSE.
//
// C8's second invariant: `kmoney_mixed` below has no `+`, no `-`, and no sum of any kind. That
// is stronger than a runtime check, because `SELECT sum(amount)` on a mixed column fails at
// PLAN time — before a row is read — rather than on row 4,000,000 of a nightly batch. It is
// the SQL analogue of `Add` existing only on `Money<C>`: the unproven form cannot be added
// because the impl is not there.
//
// `kmoney` itself has `+`, `-`, the variadic `kmoney_sum`, AND a `sum` aggregate whose
// transition state is wide. R2-F4 removed an aggregate whose state was a plain `kmoney`: it
// checked the domain on every partial total, so a running sum that transiently left the domain
// and returned failed or succeeded by plan order. The state was the defect, not the aggregate —
// see `kmoney_sum_accum` for the widened one that replaces it.
// =========================================================================================

/// The currency check both operators share.
///
/// PostgreSQL does not pass typmod to operators, so `kmoney(USD) + kmoney(IDR)` arrives
/// here as `kmoney + kmoney` with nothing but the values to tell them apart. This is the
/// only mechanism left, which is why the currency is carried in the value at all.
fn same_currency(a: kmoney, b: kmoney, op: &str) -> Iso4217 {
    if a.code() != b.code() {
        let (left, right) = (describe(a.code()), describe(b.code()));
        error!("kmoney: cannot compute {left} {op} {right}: different currencies");
    }
    let Some(currency) = Iso4217::from_numeric(a.code()) else {
        error!("kmoney: stored ISO 4217 numeric code {} is not in kamu_money_core's table", a.code());
    };
    currency
}

/// A stored numeric code resolved to a currency, or a SQL error naming the offending value.
///
/// An unknown code means the row was written by a build whose currency table differed, or the
/// bytes are corrupt. Rendering a placeholder would attach a number to the wrong currency,
/// which is precisely the silent wrongness this design exists to prevent.
///
/// One helper rather than the five hand-written copies this replaced -- two of which checked
/// `is_none()`, discarded the answer, and looked the currency up again later.
fn currency_or_error(code: u16, context: &str) -> Iso4217 {
    Iso4217::from_numeric(code).unwrap_or_else(|| {
        error!("{context}: stored ISO 4217 numeric code {code} is not in kamu_money_core's table")
    })
}

/// An ISO code for an error message, without erroring on the way to an error.
fn describe(code: u16) -> String {
    Iso4217::from_numeric(code).map_or_else(|| format!("<unknown code {code}>"), |c| c.alpha3().to_owned())
}

// =========================================================================================
mod ops;

mod aggregate;

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

#[doc(hidden)]
mod mixed;

mod division;

mod allocation;

// =========================================================================================
// THERE IS NO PATH TO `numeric`. Deliberately, and this is the second time it was removed.
//
// A `kmoney -> numeric` cast was written here and deleted the same day (operator ruling,
// 2026-07-22: "NEVER use NUMERIC"). The reasoning, recorded so it is not rediscovered:
//
//   - The cast itself is exact. That is not the problem.
//   - Its RESULT is an unconstrained `numeric`, and every PostgreSQL numeric operator is then
//     in scope -- including `*`, `/` and `avg()`, which E9 measured as silently rounding at a
//     PG-chosen, value-dependent scale. C8's claim is that the boundary rule DISAPPEARS
//     rather than being policed; a cast to numeric puts it back, invisibly.
//   - `sum(x::numeric)` is exact (E9) and `avg(x::numeric)` is not. Those are indistinguishable
//     in a BI tool's generated SQL, which makes the hazard undetectable at the call site.
//
// The whole point of this type is that the i128 never becomes a numeric: storage is the
// little-endian i128, `+`/`-` are `kamu_money_core::arith::add_units`/`sub_units` (the very kernel
// `Money::checked_add`/`checked_sub` run), and `kmoney_sum` accumulates in I256 before a single
// domain check. The compute path contains no base-10000 limbs at any point.
//
// Egress is the TEXT form -- `amount::text` gives `'USD 10.50'`: exact, carrying its currency,
// and arithmetically inert. `kamu_money_core::text::parse` reads it back with no loss.
//
// `numeric` survives in exactly two places, both of them EVIDENCE rather than code path: the
// tests below that demonstrate what `numeric(36,18)` does to over-precise input (E13) and what
// it costs per row (E14).
// =========================================================================================

// =========================================================================================
// THE BOUNDARY PROBE --- what a pgrx call costs, and nothing else.
//
// Behind `--features boundary-probe`, so none of this is in the shipped SQL surface. Adding a
// no-op to the extension to support a benchmark is a trade this workspace has not made; adding
// one that is not compiled unless a benchmark asks for it is a different trade.
//
// WHY IT IS HERE RATHER THAN IN A CONTAINER. These functions used to be APPENDED to this file
// inside a container at measurement time, from a `git archive` of the commit under test, and
// never committed. That is why E20's YugabyteDB boundary figures --- the ones that say the pgrx
// wrapper costs ~4 ns, which is the number the "why pgrx" argument rests on --- could not be
// reproduced from any revision of this repository. A figure that steers architecture has to be
// re-derivable by someone who was not there.
//
// THE MEASUREMENT ONLY WORKS IF THE SIGNATURES MATCH. `c_noop` in
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

    /// **18 bytes, on disk and in memory alike — there is no header.**
    ///
    /// This number moved three times, and each move was a measurement correcting a claim:
    ///
    /// | layout | stored | in-memory datum |
    /// |---|---:|---:|
    /// | `#[repr(C)] { i128, u16 }`, varlena | 36 | 36 |
    /// | byte arrays, varlena | 19 | 22 |
    /// | byte arrays, `INTERNALLENGTH = 18` | **18** | **18** |
    ///
    /// E12 predicted 19 from source. The first `pg_column_size` said 36 — `i128`'s 16-byte
    /// alignment had padded the struct to 32. With byte-array fields it became the predicted
    /// 19 on disk, but 22 as an expression, because a varlena carries a 4-byte header in memory
    /// and PostgreSQL only repacks it to the 1-byte short form during tuple formation.
    ///
    /// Dropping varlena entirely removes the header from both. `INTERNALLENGTH = 18` is the
    /// same shape `uuid` uses (`typlen = 16, typbyval = f, typalign = c, typstorage = p`), and
    /// it makes the two numbers equal — which is the real tell that nothing is being wrapped.
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
            "SELECT format('%s/%s/%s/%s', typlen, typbyval, typalign, typstorage)
               FROM pg_type WHERE typname = 'kmoney'",
        )
        .expect("query ran")
        .expect("not null");
        // PostgreSQL renders booleans as t/f, so typbyval = false prints as "f".
        assert_eq!(row, "18/f/c/p");
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
    /// An earlier version of this test compared *only* at the domain top, where `kmoney`
    /// wins, and passed. A test that measures the one favourable point is not evidence. What
    /// C8 actually buys is in the other tests here: a value that cannot be stored without its
    /// currency, a width that does not move with the data, and a refusal of the precision
    /// `numeric` silently swallows (E13). Space is not on the list.
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
             {typical_r}. If this ever reverses, the size argument in C8 needs remeasuring, \
             not restating."
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

    /// The text form round-trips through the database unchanged, and it is the SAME form
    /// `kamu_money_core` renders — one format across Rust and SQL, per §0.1.
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

    /// Half of E13: PostgreSQL itself loses the value, silently, with `INSERT 0 1`.
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
        assert!(rounded, "E13: numeric(36,18) rounds 4e-19 to zero, silently");
    }

    /// `kmoney` refuses exactly what `numeric(36,18)` swallows.
    ///
    /// This is the difference an input function makes: it runs BEFORE any coercion, where a
    /// `CHECK` or `DOMAIN` runs after and is handed the already-altered value.
    // The expected message is one long line on purpose: pgrx parses this attribute by taking
    // the literal's source text and running it through an `unescape` pass, so a `\`-newline
    // continuation is not reliably folded away and would be compared verbatim.
    #[pg_test(
        error = "kmoney: 19 fractional digits exceeds the canonical scale of 18; refused rather than rounded, because rounding here would lose money silently, in \"USD 0.0000000000000000004\""
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
        error = "kmoney: money domain overflow: 1000000000000000000000000000000000000 units is outside the domain |units| <= 999999999999999999999999999999999999 (NUMERIC(36,18) admits |v| < 10^18), in \"IDR 1000000000000000000\""
    )]
    fn one_unit_past_the_domain_is_refused() {
        Spi::get_one::<String>("SELECT 'IDR 1000000000000000000'::kmoney::text").ok();
    }

    /// A currency `kamu_money_core` does not know is refused at input, not stored and guessed at
    /// later. There is exactly one currency table and it lives in `kamu_money_core` (C9).
    #[pg_test(error = "kmoney: not a money literal: expected \"<ISO> <amount>\", in \"ZWL 1.00\"")]
    fn an_unknown_currency_is_refused_at_the_boundary() {
        Spi::get_one::<String>("SELECT 'ZWL 1.00'::kmoney::text").ok();
    }

    /// There is no route from `kmoney` to `numeric`, and that is load-bearing rather than
    /// an omission: a bare `numeric` would put every silently-rounding PostgreSQL operator
    /// back in scope (E9). The text form is the egress.
    /// **THE PHASE 4 <-> PHASE 5 DIFFERENTIAL.**
    ///
    /// Phase 4 stores money as the canonical text form in a `text` column, on any PostgreSQL.
    /// Phase 5 stores it as this native 18-byte type. They are different storage strategies
    /// for the same value, and the whole "one codec" claim rests on them agreeing.
    ///
    /// Both columns are written from the SAME literal here, and the assertion is that
    /// `kmoney`'s output function reproduces the text form exactly. If `kamu_money_core::text` and
    /// this extension's in/out functions ever diverge, an application reading through
    /// `money-postgres` and a query reading the native column would return different numbers
    /// for the same row -- which is the failure §0.1 exists to make impossible.
    #[pg_test]
    fn the_native_type_and_the_text_storage_agree() {
        Spi::run(
            "CREATE TABLE both_forms (
                 phase4 text    NOT NULL,   -- what money-postgres / money-sqlx write
                 phase5 kmoney  NOT NULL    -- what this extension stores
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
            Spi::get_one::<i64>("SELECT count(*) FROM both_forms WHERE phase4 <> phase5::text")
                .expect("query ran")
                .expect("not null");
        assert_eq!(disagreements, 0, "the text storage and the native type must render identically");

        // And the reverse direction: text parsed into the native type equals the native value.
        let mismatches =
            Spi::get_one::<i64>("SELECT count(*) FROM both_forms WHERE phase4::kmoney::text <> phase5::text")
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
