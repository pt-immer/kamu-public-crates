//! Binary `SEND`/`RECEIVE`: the 18-byte payload on the wire, and the raw recv FFI.
//!
//! Needed because tokio-postgres and sqlx request BINARY result format by default, so a Rust
//! client reading a native column hits this immediately -- while every in-backend `#[pg_test]`
//! speaks the text protocol, which is exactly why nothing here noticed for so long.
//!
//! `recv` cannot be a plain `#[pg_extern]`: it takes `internal` (a `StringInfo`), which pgrx has
//! no safe mapping for. Binary input is NO LESS UNTRUSTED than text input, so recv performs the
//! same two checks the text input function does rather than believing 18 bytes it was handed.

use pgrx::datum::IntoDatum;
use pgrx::prelude::*;

use crate::safe::payload::{PAYLOAD_BYTES, Payload, ValidationError, validate_payload};
use crate::safe::validated_or_error;
use crate::{kmoney, kmoney_mixed};

// BINARY I/O: `SEND` and `RECEIVE`.
//
// Without these a client that asks for binary result format gets
// `no binary output function available for type kmoney` -- and tokio-postgres and sqlx request
// binary for result columns BY DEFAULT, so a Rust program reading a native column hit it
// immediately. Every test in this crate is an in-database `#[pg_test]` speaking the text
// protocol, which is exactly why nothing here noticed.
//
// The wire form is the same 18-byte payload the type stores: `[u8; 16]` units little-endian,
// then `[u8; 2]` ISO numeric code. It is already endian-explicit, so send is a copy and recv
// is a copy plus the two checks the text input function also performs. That is deliberate --
// binary input is no less untrusted than text input, and a client that sends 18 bytes of
// garbage must be refused rather than believed.
//
// `send` is an ordinary `#[pg_extern]`. `recv` cannot be: it takes `internal` (a `StringInfo`),
// which pgrx 0.19.1 has no safe mapping for -- the same wall `typmod_in`'s `cstring[]` hit, and
// it takes the same remedy: a raw `#[pg_guard] extern "C-unwind"` function with a hand-written
// finfo record, declared through `extension_sql!` by symbol name.

/// `send(kmoney) -> bytea` — the stored payload, verbatim.
/// Requires only the SHELL type, exactly as the in/out functions do. Depending on
/// `*_concrete` would be a CYCLE -- `CREATE TYPE ... SEND = kmoney_send` needs this
/// function to already exist, while this function would be waiting for that CREATE TYPE.
/// PostgreSQL reports the losing side of it as
/// `function kmoney_mixed_send(kmoney_mixed) does not exist`, which names the symptom
/// and not the cycle.
#[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
fn kmoney_send(value: kmoney) -> Vec<u8> {
    validated_or_error(value.payload(), "kmoney").payload().to_bytes().to_vec()
}

/// `send(kmoney_mixed) -> bytea`.
#[pg_extern(immutable, parallel_safe, requires = ["money_shell_types"])]
fn kmoney_mixed_send(value: kmoney_mixed) -> Vec<u8> {
    validated_or_error(value.payload(), "kmoney_mixed").payload().to_bytes().to_vec()
}

/// Read an 18-byte payload off the wire, validating it exactly as the text path validates.
///
/// # Safety
/// Called only by PostgreSQL through a `RECEIVE` slot, which guarantees `fcinfo` is valid and
/// argument 0 is a non-null `internal` pointing at a `StringInfo`.
unsafe fn recv_payload(fcinfo: pg_sys::FunctionCallInfo, context: &str) -> Payload {
    // A REAL check, not `debug_assert!`. fmgr guarantees `nargs` for a RECEIVE slot, so a
    // mis-arity here is a registration or catalog mistake rather than user input -- but
    // `debug_assert!` compiles OUT of the release build, and the release build is the one handling
    // real money. What follows is a manual index into a flexible array, so the profile that
    // skipped the check was the profile that turned a catalog mistake into an unchecked read.
    // This crate has already learned that lesson once, from a residue drop-bomb that panicked in
    // debug and merely counted in release. One integer compare, and the error is an ereport.
    // SAFETY: this function's PostgreSQL contract guarantees `fcinfo` points to valid call data.
    if unsafe { (*fcinfo).nargs } < 1 {
        error!("{context}: RECEIVE called with no argument");
    }
    // SAFETY: PostgreSQL populates `args` for every call through this slot.
    let arg = unsafe { (*fcinfo).args.as_ptr().read().value };
    let buf = arg.cast_mut_ptr::<pg_sys::StringInfoData>();

    let mut bytes = [0u8; PAYLOAD_BYTES];
    // `try_from` rather than `as`: 18 fits `c_int` on every supported platform so this cannot
    // fire, but an `as` here would silently truncate if the payload width ever grew, and a
    // truncated length passed to pq_copymsgbytes would under-fill `payload` and leave the tail
    // of a money value uninitialised.
    let want = core::ffi::c_int::try_from(PAYLOAD_BYTES)
        .expect("PAYLOAD_BYTES is 18, which fits c_int on every supported platform");
    // `pq_copymsgbytes` rather than `pq_getmsgbytes`: it copies into our own buffer, so no
    // reference is ever constructed over the message's memory. It raises if the message is
    // short, which is the correct answer to a truncated payload.
    // Pinned by `recv_refuses_a_truncated_binary_payload`.
    //
    // SAFETY: `buf` is the StringInfo PostgreSQL passed; `payload` is exactly PAYLOAD_BYTES.
    unsafe {
        pg_sys::pq_copymsgbytes(
            buf,
            // Untyped `.cast()`: pq_copymsgbytes's buffer parameter is `*mut c_void` on
            // PG18 and `*mut c_char` on PG15, so a turbofish compiles on one major and
            // fails on another. Inference picks whichever this major declares.
            bytes.as_mut_ptr().cast(),
            want,
        );
        // Refuse trailing bytes. Without this, a longer message would be silently accepted and
        // its tail ignored -- the shape of every "we read what we expected and moved on" bug.
        // Pinned by `recv_refuses_a_binary_payload_with_trailing_bytes`.
        pg_sys::pq_getmsgend(buf);
    }

    let payload = Payload::from_bytes(bytes);
    if let Err(error) = validate_payload(payload, None) {
        match error {
            ValidationError::OutOfDomain { currency, .. } => {
                error!(
                    "{context}: received {} amount is outside the domain |units| <= 10^36 - 1",
                    currency.alpha3()
                );
            }
            ValidationError::UnknownCurrency { .. } | ValidationError::UnexpectedCurrency { .. } => {
                error!("{context}: {error}");
            }
        }
    }
    payload
}

/// `recv(internal) -> kmoney`.
///
/// # Safety
/// See `recv_payload`, which is private -- a plain code span rather than an intra-doc link,
/// because a public item linking to a private one is a rustdoc error and would break the
/// docs.rs build of a crate this repository's gate had just declared releasable.
#[unsafe(no_mangle)]
#[pg_guard]
pub unsafe extern "C-unwind" fn kmoney_recv(fcinfo: pg_sys::FunctionCallInfo) -> pg_sys::Datum {
    // SAFETY: this entry point has the same RECEIVE contract and forwards `fcinfo` unchanged.
    let payload = unsafe { recv_payload(fcinfo, "kmoney") };
    kmoney::from_payload(payload)
        .into_datum()
        .unwrap_or_else(|| error!("kmoney: could not allocate a received value"))
}

/// `recv(internal) -> kmoney_mixed`.
///
/// # Safety
/// See `recv_payload`, which is private -- a plain code span rather than an intra-doc link,
/// because a public item linking to a private one is a rustdoc error and would break the
/// docs.rs build of a crate this repository's gate had just declared releasable.
#[unsafe(no_mangle)]
#[pg_guard]
pub unsafe extern "C-unwind" fn kmoney_mixed_recv(fcinfo: pg_sys::FunctionCallInfo) -> pg_sys::Datum {
    // SAFETY: this entry point has the same RECEIVE contract and forwards `fcinfo` unchanged.
    let payload = unsafe { recv_payload(fcinfo, "kmoney_mixed") };
    kmoney_mixed::from_payload(payload)
        .into_datum()
        .unwrap_or_else(|| error!("kmoney_mixed: could not allocate a received value"))
}

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use std::path::{Path, PathBuf};

    use pgrx::prelude::*;
    use tempfile::TempDir;

    struct BinaryCopy {
        _directory: TempDir,
        bytes: Vec<u8>,
        bad: PathBuf,
    }

    fn temporary_directory(tag: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(&format!("kmoney-{tag}-"))
            .tempdir_in("/tmp")
            .expect("created a private temporary directory")
    }

    fn sql_path(path: &Path) -> String {
        path.to_str().expect("temporary path is valid UTF-8").replace('\'', "''")
    }

    /// An unpinned column still takes anything: typmod -1 is "no modifier", not "no currency".
    /// Binary I/O round-trips, and refuses what the text path refuses.
    ///
    /// `send`/`recv` existed nowhere until an idiomatic review pointed out that a client
    /// requesting binary result format — which tokio-postgres and sqlx do BY DEFAULT — could
    /// not read this type at all. Nothing here noticed because every test in this file speaks
    /// the text protocol.
    #[pg_test]
    fn the_binary_wire_round_trips_and_is_not_more_trusted_than_text() {
        // NOTE: `recv` cannot be called directly from SQL -- its argument is `internal`, which
        // has no SQL literal, and that is a deliberate PostgreSQL restriction rather than a gap
        // here. So recv is exercised the way a real binary-protocol client reaches it: the
        // `COPY ... (FORMAT BINARY)` round trip below drives `kmoney_send` on the way out and
        // `kmoney_recv` on the way back in. An earlier draft used `INSERT ... SELECT`, which
        // copies internal datums and calls neither, so it proved nothing about the wire (R2-F5).

        Spi::run("CREATE TABLE bin_io (amount kmoney)").expect("table created");
        Spi::run("INSERT INTO bin_io VALUES ('IDR -16000.50'), ('USD 0.000000000000000001')")
            .expect("rows inserted");

        // The catalog must advertise both, or a binary-format client falls back or fails.
        let binary_ready = Spi::get_one::<bool>(
            "SELECT count(*) = 2 AND bool_and(typsend <> 0) AND bool_and(typreceive <> 0)
               FROM pg_type WHERE typname IN ('kmoney', 'kmoney_mixed')",
        )
        .expect("query ran")
        .expect("row");
        assert!(binary_ready, "both money types must declare SEND and RECEIVE");

        // send produces exactly the 18 stored bytes.
        let widths = Spi::get_one::<String>(
            "SELECT format(
                 '%s/%s',
                 octet_length(kmoney_send('USD 1.00'::kmoney)),
                 octet_length(kmoney_mixed_send('USD 1.00'::kmoney_mixed))
             )",
        )
        .expect("query ran")
        .expect("row");
        assert_eq!(widths, "18/18", "both binary forms are the stored payload");

        // COPY (FORMAT BINARY) out and back in is the real client path: it calls `kmoney_send`
        // writing the file and `kmoney_recv` reading it -- the two functions the catalog just
        // promised. `TempDir` gives each test a private path and removes it on drop.
        let directory = temporary_directory("roundtrip");
        let path = directory.path().join("wire.bin");
        let path_sql = sql_path(&path);
        Spi::run(&format!("COPY bin_io TO '{path_sql}' (FORMAT BINARY)")).expect("send: COPY out");
        Spi::run("CREATE TABLE bin_copy (LIKE bin_io)").expect("table created");
        Spi::run(&format!("COPY bin_copy FROM '{path_sql}' (FORMAT BINARY)")).expect("recv: COPY in");

        // recv must reconstruct the exact payload. A `JOIN USING(amount)` can NOT detect a
        // value-corrupting recv: a mangled row fails to pair and drops out of the inner join,
        // leaving the mismatch count at zero -- a tautology. Compare the ORDERED text projections
        // of both tables instead; any single corrupted byte changes a text form and breaks the
        // array equality.
        let intact = Spi::get_one::<bool>(
            "SELECT (SELECT array_agg(amount::text ORDER BY amount::text) FROM bin_io)
                  = (SELECT array_agg(amount::text ORDER BY amount::text) FROM bin_copy)",
        )
        .expect("query ran")
        .expect("row");
        assert!(intact, "the binary wire round trip changed a value");
        let copied = Spi::get_one::<i64>("SELECT count(*) FROM bin_copy").expect("query ran").expect("row");
        assert_eq!(copied, 2, "both rows must survive send -> recv");
    }

    /// recv is NOT more trusted than text: a binary payload whose units are out of domain is
    /// refused, proving recv's validation branch runs on the wire (R2-F5). PostgreSQL writes the
    /// COPY framing (so the file is well-formed); we corrupt only the 18-byte `kmoney` field in
    /// place -- overwriting the little-endian units and leaving the currency code valid, so it is
    /// the DOMAIN check that fires with a kamu_money_core-owned (version-stable) message.
    #[pg_test(error = "kmoney: received USD amount is outside the domain |units| <= 10^36 - 1")]
    fn recv_refuses_an_out_of_domain_binary_payload() {
        let mut copy = binary_copy_of("USD 1.00", "kmoney", "domain");
        // 10^36 is one past the domain top (|units| <= 10^36 - 1). Overwrite the 16-byte LE units;
        // bytes 41..43 (the currency code = USD) are left intact so the domain check is what fires.
        let out_of_domain: i128 = 1_000_000_000_000_000_000_000_000_000_000_000_000;
        copy.bytes[25..41].copy_from_slice(&out_of_domain.to_le_bytes());
        recv_bytes("recv_bad", "kmoney", &copy.bad, &copy.bytes);
    }

    // -----------------------------------------------------------------------------------
    // recv is the ONLY path that takes attacker-shaped bytes straight into an unsafe FFI
    // function, so its refusals are pinned one at a time below. PostgreSQL writes the COPY
    // framing so the file is always well-formed; each test then crafts exactly one defect.
    //
    // One-row one-column BINARY COPY layout: 11 signature + 4 flags + 4 header-extension +
    // 2 field-count + 4 field-length + 18 payload + 2 trailer = 45 bytes. So the field length
    // is `[21..25]` (int32, NETWORK order), the payload `[25..43]` (16 LE units, then the
    // 2 LE ISO code), and the trailer `[43..]`.
    // -----------------------------------------------------------------------------------

    /// Write one valid single-row BINARY COPY file in a private, self-cleaning directory.
    fn binary_copy_of(literal: &str, sql_type: &str, tag: &str) -> BinaryCopy {
        let directory = temporary_directory(tag);
        let good = directory.path().join("good.bin");
        let bad = directory.path().join("bad.bin");
        Spi::run(&format!("COPY (SELECT '{literal}'::{sql_type}) TO '{}' (FORMAT BINARY)", sql_path(&good)))
            .expect("wrote a valid binary payload");
        let bytes = std::fs::read(&good).expect("read the good payload");
        BinaryCopy { _directory: directory, bytes, bad }
    }

    /// Feed crafted bytes back in as a BINARY COPY, so the type's recv function runs on them.
    fn recv_bytes(table: &str, sql_type: &str, path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("wrote the crafted payload");
        Spi::run(&format!("CREATE TABLE {table} (amount {sql_type})")).expect("table created");
        Spi::run(&format!("COPY {table} FROM '{}' (FORMAT BINARY)", sql_path(path))).ok();
    }

    /// recv must REFUSE a short message, not under-fill its buffer. This is the memory-safety
    /// half of the recv contract: `payload` is a fixed `[u8; 18]`, and if `pq_copymsgbytes` did
    /// not raise when fewer than 18 bytes remain, the tail would stay UNINITIALISED and be
    /// reinterpreted as units — money read out of whatever happened to be in that memory.
    /// `recv_payload` asserts this in a comment; this is the proof.
    ///
    /// The field length is cut to 10 AND the payload truncated to match, so the file stays
    /// self-consistent and it is recv, not COPY's own framing check, that refuses.
    #[pg_test(error = "insufficient data left in message")]
    fn recv_refuses_a_truncated_binary_payload() {
        let copy = binary_copy_of("USD 1.00", "kmoney", "short");
        let mut short = copy.bytes[..21].to_vec();
        short.extend_from_slice(&10_i32.to_be_bytes());
        short.extend_from_slice(&copy.bytes[25..35]);
        short.extend_from_slice(&copy.bytes[43..]);
        recv_bytes("recv_short", "kmoney", &copy.bad, &short);
    }

    /// recv must REFUSE trailing bytes rather than take the 18 it wanted and ignore the rest —
    /// "we read what we expected and moved on" is how a re-framed or version-skewed payload gets
    /// silently accepted as a different amount.
    ///
    /// The expected message is `pq_getmsgend`'s, NOT COPY's, and that distinction is the whole
    /// point: delete the `pq_getmsgend` call and PostgreSQL's own post-check still errors, but
    /// with "improper binary format in file". So this goes red on exactly the guard it pins.
    #[pg_test(error = "invalid message format")]
    fn recv_refuses_a_binary_payload_with_trailing_bytes() {
        let copy = binary_copy_of("USD 1.00", "kmoney", "long");
        let mut long = copy.bytes[..21].to_vec();
        long.extend_from_slice(&26_i32.to_be_bytes());
        long.extend_from_slice(&copy.bytes[25..43]);
        long.extend_from_slice(&[0_u8; 8]);
        long.extend_from_slice(&copy.bytes[43..]);
        recv_bytes("recv_long", "kmoney", &copy.bad, &long);
    }

    /// Binary is not more trusted than text, and BOTH of recv's checks have to prove it. The
    /// domain half is pinned by `recv_refuses_an_out_of_domain_binary_payload`; this is the
    /// currency half. The units are left valid and only the 2-byte ISO code is overwritten with
    /// 0, which is not an assigned ISO 4217 numeric code.
    #[pg_test(error = "kmoney: stored ISO 4217 numeric code 0 is not in kamu_money_core's table")]
    fn recv_refuses_a_binary_payload_whose_currency_is_unknown() {
        let mut copy = binary_copy_of("USD 1.00", "kmoney", "nocur");
        copy.bytes[41..43].copy_from_slice(&0_u16.to_le_bytes());
        recv_bytes("recv_nocur", "kmoney", &copy.bad, &copy.bytes);
    }

    /// `kmoney_mixed_recv` is a SECOND `no_mangle` FFI entry point. It shares `recv_payload`,
    /// which is exactly what this pins: the mixed symbol must route through the same validation
    /// rather than accept bytes the strict type would reject.
    #[pg_test(error = "kmoney_mixed: received USD amount is outside the domain |units| <= 10^36 - 1")]
    fn the_mixed_recv_entry_point_validates_too() {
        let mut copy = binary_copy_of("USD 1.00", "kmoney_mixed", "mixed");
        let out_of_domain: i128 = 1_000_000_000_000_000_000_000_000_000_000_000_000;
        copy.bytes[25..41].copy_from_slice(&out_of_domain.to_le_bytes());
        recv_bytes("recv_mixed", "kmoney_mixed", &copy.bad, &copy.bytes);
    }
}
