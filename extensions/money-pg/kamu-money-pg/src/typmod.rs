//! `kmoney('IDR')`: the raw typmod FFI, the coercion cast, and the `CREATE TYPE`.
//!
//! Split out of `lib.rs` on 2026-07-27. The code is UNCHANGED -- this file is
//! a relocation, verified by `just schema-hash` (the SQL surface) and `just test-pg` (behaviour).
//!
//! RAW `extern "C"` because pgrx 0.19.1 cannot express these signatures: PostgreSQL hands
//! `typmod_in` a `cstring[]` and pgrx has no safe mapping for it. `#[unsafe(no_mangle)]` is
//! load-bearing -- the hand-written SQL binds these by SYMBOL NAME, so the symbol must survive
//! this move, and it does because a module path never reaches a linker name.
//!
//! Typmod does NOT make cross-currency arithmetic fail: PostgreSQL does not pass typmod to
//! operators, so the value-carried code stays the only thing separating two currencies in an
//! expression. This is a column-level INSERT/coercion check and nothing more.

use super::{describe, kmoney};
use kamu_money_core::Iso4217;
use pgrx::prelude::*;

/// `typmod_in(cstring[]) -> int4` — parse `kmoney('IDR')` into the ISO numeric code.
///
/// # Safety
/// Called only by PostgreSQL through the `TYPMOD_IN` slot, which guarantees `fcinfo` is valid
/// and argument 0 is a non-null `cstring[]`.
// `no_mangle` is required: the SQL below binds this by symbol name, and without it
// Rust mangles it away -- PostgreSQL then reports
// `could not find function "kmoney_typmod_in" in file "...kmoney.so"`.
#[unsafe(no_mangle)]
#[pg_guard]
pub unsafe extern "C-unwind" fn kmoney_typmod_in(
    fcinfo: pg_sys::FunctionCallInfo,
) -> pg_sys::Datum {
    // A REAL check, not `debug_assert!` -- see `recv_payload` for the full argument. The release
    // build must not be the permissive one when the next statement indexes a flexible array.
    if unsafe { (*fcinfo).nargs } < 1 {
        error!("kmoney_typmod_in: called with no argument");
    }
    // SAFETY: PostgreSQL populates `args` for every call through this slot.
    let arg = unsafe { (*fcinfo).args.as_ptr().read().value };
    // `pg_detoast_datum` first, matching what PG_GETARG_ARRAYTYPE_P does for every built-in
    // typmodin (`numerictypmodin`, `intervaltypmodin`, ...). A short-header or compressed
    // datum would otherwise have its `ndim`/dims read out of garbage.
    //
    // Not reachable today -- `typenameTypeMod` builds this array with
    // `construct_array_builtin`, and `cstring[]` is a pseudo-type so it can never be stored
    // and therefore never TOASTed -- but it is one call, and matching the convention costs
    // nothing while depending on that reasoning costs a rare, unreproducible crash.
    //
    // SAFETY: `arg` is the datum PostgreSQL passed for a `cstring[]` parameter.
    //
    // clippy::cast_ptr_alignment fires here because Rust models `varlena` as alignment 1 while
    // `ArrayType` wants 4. It is a false positive, and the reason is worth writing down rather
    // than silencing globally: `pg_detoast_datum` returns palloc'd memory, palloc is MAXALIGN'd
    // (8 or 16 bytes), so the pointer is OVER-aligned for ArrayType, never under. This is the
    // same cast PostgreSQL's own DatumGetArrayTypeP macro performs.
    #[allow(clippy::cast_ptr_alignment)]
    let array = unsafe { pg_sys::pg_detoast_datum(arg.cast_mut_ptr()) }.cast::<pg_sys::ArrayType>();

    // `try_from` rather than `as`: `c_char` is signed on x86-64 and unsigned on some ARM
    // targets, so `as` would be a platform-dependent wrap. ASCII 'c' (99) fits either way.
    let typalign_char = core::ffi::c_char::try_from(b'c')
        .expect("TYPALIGN_CHAR is ASCII 'c' (99), which fits c_char signed or unsigned");

    let mut count: core::ffi::c_int = 0;
    let mut items: *mut pg_sys::Datum = core::ptr::null_mut();
    // `deconstruct_array`, NOT `deconstruct_array_builtin`. The `_builtin` wrapper is a PG15
    // convenience that looks up the element type's len/byval/align from a hardcoded table of
    // built-in types -- and a PostgreSQL FORK need not carry it: YugabyteDB's PG15 omits the
    // symbol, so `deconstruct_array_builtin` fails to link there while every other symbol in
    // this file resolves. The primitive `deconstruct_array` takes the descriptor explicitly and
    // exists in every PostgreSQL and every fork. For `cstring` the values are catalog-stable
    // (pg_type: typlen = -2, typbyval = f, typalign = 'c'), so passing them by hand is exact,
    // not a guess. This is the same function PostgreSQL's own typmod_in implementations reduce
    // to; elements come back as Datums, each of which for a cstring IS the char pointer.
    unsafe {
        pg_sys::deconstruct_array(
            array,
            pg_sys::CSTRINGOID,
            -2,            // cstring typlen
            false,         // cstring typbyval
            typalign_char, // cstring typalign = TYPALIGN_CHAR
            &raw mut items,
            core::ptr::null_mut(),
            &raw mut count,
        );
    }

    if count != 1 {
        error!("kmoney: expected exactly one type modifier, as in kmoney('IDR'); got {count}");
    }

    // SAFETY: `deconstruct_array` wrote `count` valid cstring Datums into `items`, and
    // `count == 1` was just checked.
    let raw =
        unsafe { core::ffi::CStr::from_ptr(items.read().cast_mut_ptr::<core::ffi::c_char>()) };
    let Ok(alpha3) = raw.to_str() else {
        error!("kmoney: type modifier is not valid UTF-8");
    };

    let Some(currency) = Iso4217::from_alpha3(alpha3) else {
        error!("kmoney: {alpha3:?} is not an ISO 4217 code kamu_money_core knows");
    };

    pg_sys::Datum::from(i32::from(currency.numeric()))
}

/// `typmod_out(int4) -> cstring` — render the code back as `('IDR')` for `\d` and `pg_dump`.
///
/// # Safety
/// Called only by PostgreSQL through the `TYPMOD_OUT` slot.
// `no_mangle` is required: the SQL below binds this by symbol name, and without it
// Rust mangles it away -- PostgreSQL then reports
// `could not find function "kmoney_typmod_out" in file "...kmoney.so"`.
#[unsafe(no_mangle)]
#[pg_guard]
pub unsafe extern "C-unwind" fn kmoney_typmod_out(
    fcinfo: pg_sys::FunctionCallInfo,
) -> pg_sys::Datum {
    // A REAL check, not `debug_assert!` -- see `recv_payload` for the full argument.
    if unsafe { (*fcinfo).nargs } < 1 {
        error!("kmoney_typmod_out: called with no argument");
    }
    // SAFETY: PostgreSQL populates `args` for every call through this slot.
    // PostgreSQL stores "no typmod" as the sentinel -1, which reaches here as
    // 0xFFFF_FFFF_FFFF_FFFF in the Datum's usize. `as` reinterprets the low 32 bits and recovers
    // -1 exactly, whereas `i32::try_from` would REJECT it as out of range. This is the rare cast
    // where the lint's suggested "safe" fix is the bug, so the exception is taken narrowly on
    // this one statement rather than on the whole function.
    #[allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    let typmod = unsafe { (*fcinfo).args.as_ptr().read().value.value() } as i32;

    // A dumped schema must round-trip through typmod_in, so an unknown code cannot be rendered
    // as something that would parse back to a different currency.
    let rendered =
        if let Some(currency) = u16::try_from(typmod).ok().and_then(Iso4217::from_numeric) {
            format!("('{}')", currency.alpha3())
        } else {
            error!("kmoney: stored type modifier {typmod} is not an ISO 4217 numeric code")
        };

    // SAFETY: CurrentMemoryContext is valid; PostgreSQL frees this with the context.
    let size = rendered
        .len()
        .checked_add(1)
        .expect("a 7-byte typmod render plus its NUL terminator cannot overflow usize");
    // SAFETY: CurrentMemoryContext is valid; PostgreSQL frees this with the context.
    let out = unsafe { pg_sys::palloc(size).cast::<u8>() };
    // SAFETY: `out` has room for the bytes plus the terminator.
    unsafe {
        core::ptr::copy_nonoverlapping(rendered.as_ptr(), out, rendered.len());
        out.add(rendered.len()).write(0);
    }
    pg_sys::Datum::from(out)
}

/// The coercion cast: check a value against the column's declared currency.
///
/// This is what makes `INSERT INTO ledger VALUES ('USD 1.00')` fail on an `kmoney('IDR')`
/// column. It is the SQL twin of a value being proved into `Money<IDR>`.
#[pg_extern(immutable, parallel_safe, requires = ["kmoney_concrete"])]
fn kmoney_coerce(value: kmoney, typmod: i32, _is_explicit: bool) -> kmoney {
    // -1 is PostgreSQL's "no modifier": an unpinned kmoney column accepts any currency.
    if typmod == -1 {
        return value;
    }
    let Some(want) = u16::try_from(typmod).ok().and_then(Iso4217::from_numeric) else {
        error!("kmoney: column type modifier {typmod} is not an ISO 4217 numeric code");
    };
    if value.code() != want.numeric() {
        error!(
            "kmoney: column is declared kmoney('{}') but the value is {}",
            want.alpha3(),
            describe(value.code())
        );
    }
    value
}

// The real type. `INTERNALLENGTH = 18` rather than `variable` is the whole point: no varlena
// header, so 18 bytes on disk instead of 19. `ALIGNMENT = char` because the payload is
// byte-arrays (align_of == 1, asserted in the macro), and `STORAGE = plain` because an 18-byte
// value must never be considered for TOAST.
//
// The typmod functions are declared here rather than by `#[pg_extern]` because their signatures
// (`cstring[]`, and a bare `cstring` return) are outside what pgrx can generate.
extension_sql!(
    r"
CREATE FUNCTION kmoney_typmod_in(cstring[]) RETURNS integer
    AS 'MODULE_PATHNAME', 'kmoney_typmod_in'
    LANGUAGE c IMMUTABLE STRICT PARALLEL SAFE;

CREATE FUNCTION kmoney_typmod_out(integer) RETURNS cstring
    AS 'MODULE_PATHNAME', 'kmoney_typmod_out'
    LANGUAGE c IMMUTABLE STRICT PARALLEL SAFE;

CREATE FUNCTION kmoney_recv(internal) RETURNS kmoney
    AS 'MODULE_PATHNAME', 'kmoney_recv'
    LANGUAGE C IMMUTABLE STRICT PARALLEL SAFE;

CREATE TYPE kmoney (
    INTERNALLENGTH = 18,
    INPUT          = kmoney_in,
    OUTPUT         = kmoney_out,
    SEND           = kmoney_send,
    RECEIVE        = kmoney_recv,
    TYPMOD_IN      = kmoney_typmod_in,
    TYPMOD_OUT     = kmoney_typmod_out,
    ALIGNMENT      = char,
    STORAGE        = plain
);
",
    name = "kmoney_concrete",
    requires = [kmoney_send, "money_shell_types", kmoney_in, kmoney_out],
);

// The length-coercion cast PostgreSQL invokes when a value lands in a typmod-bearing column.
// `AS IMPLICIT` is required: without it PostgreSQL never calls the function and the column's
// declared currency is decorative.
extension_sql!(
    r"
CREATE CAST (kmoney AS kmoney) WITH FUNCTION kmoney_coerce(kmoney, integer, boolean) AS IMPLICIT;
",
    name = "kmoney_typmod_cast",
    requires = ["kmoney_concrete", kmoney_coerce],
);

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    // -----------------------------------------------------------------------------------
    // typmod: kmoney('IDR') pins a column to one currency.
    // -----------------------------------------------------------------------------------

    /// A pinned column accepts its own currency and reports the modifier back through
    /// `format_type`, which is what `\d` and `pg_dump` read.
    #[pg_test]
    fn a_typmod_column_round_trips_its_currency() {
        Spi::run("CREATE TABLE pinned (amount kmoney('IDR'))").expect("table created");
        Spi::run("INSERT INTO pinned VALUES ('IDR 16000.00')").expect("row inserted");

        let stored = Spi::get_one::<String>("SELECT amount::text FROM pinned")
            .expect("query ran")
            .expect("not null");
        assert_eq!(stored, "IDR 16000.00");

        let declared = Spi::get_one::<String>(
            "SELECT format_type(atttypid, atttypmod) FROM pg_attribute
              WHERE attrelid = 'pinned'::regclass AND attname = 'amount'",
        )
        .expect("query ran")
        .expect("not null");
        assert_eq!(
            declared, "kmoney('IDR')",
            "typmod_out must round-trip for pg_dump"
        );
    }

    /// **The point of typmod.** The wrong currency is refused at INSERT, before it is stored --
    /// the SQL twin of proving a value into `Money<IDR>`.
    #[pg_test(error = "kmoney: column is declared kmoney('IDR') but the value is USD")]
    fn a_typmod_column_refuses_the_wrong_currency() {
        Spi::run("CREATE TABLE pinned_reject (amount kmoney('IDR'))").expect("table created");
        Spi::get_one::<String>("INSERT INTO pinned_reject VALUES ('USD 1.00')").ok();
    }

    #[pg_test]
    fn an_unpinned_column_still_accepts_every_currency() {
        Spi::run("CREATE TABLE unpinned (amount kmoney)").expect("table created");
        Spi::run("INSERT INTO unpinned VALUES ('USD 1.00'), ('IDR 16000.00')")
            .expect("rows inserted");
        let n = Spi::get_one::<i64>("SELECT count(*) FROM unpinned")
            .expect("query ran")
            .expect("not null");
        assert_eq!(n, 2);
    }

    #[pg_test(error = "kmoney: \"ZWL\" is not an ISO 4217 code kamu_money_core knows")]
    fn a_typmod_of_an_unknown_currency_is_refused() {
        Spi::run("CREATE TABLE bad_typmod (amount kmoney('ZWL'))").ok();
    }

    #[pg_test(error = "kmoney: expected exactly one type modifier, as in kmoney('IDR'); got 2")]
    fn two_type_modifiers_are_refused() {
        Spi::run("CREATE TABLE two_mods (amount kmoney('IDR', 'USD'))").ok();
    }

    /// **Typmod does NOT reach operators**, which C8 measured and this pins: two differently
    /// pinned columns still meet as bare `kmoney + kmoney`, so the refusal comes from the
    /// value-carried code rather than from the column types. Both mechanisms are required.
    #[pg_test(error = "kmoney: cannot compute IDR + USD: different currencies")]
    fn typmod_does_not_reach_operators_so_the_value_check_still_fires() {
        Spi::run("CREATE TABLE lhs (amount kmoney('IDR'))").expect("table created");
        Spi::run("CREATE TABLE rhs (amount kmoney('USD'))").expect("table created");
        Spi::run("INSERT INTO lhs VALUES ('IDR 1.00')").expect("row inserted");
        Spi::run("INSERT INTO rhs VALUES ('USD 1.00')").expect("row inserted");
        Spi::get_one::<String>("SELECT (l.amount + r.amount)::text FROM lhs l, rhs r").ok();
    }
}
