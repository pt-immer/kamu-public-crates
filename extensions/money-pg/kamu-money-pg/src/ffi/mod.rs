//! PostgreSQL ABI boundary.
//!
//! SQL registration fixes both money types at 18 bytes, pass-by-reference,
//! byte-aligned, and plain storage. For their registered OIDs, a non-null
//! `Datum` therefore points to at least 18 readable bytes; array elements use
//! the same representation. pgrx invokes the ABI traits only for those SQL
//! types. PostgreSQL errors may escape through pgrx's panic machinery, so no
//! critical cleanup is delegated to a destructor after an error.

mod datum;
mod typmod;
mod wire;

/// Emit PostgreSQL's V1 calling-convention record for one raw symbol.
macro_rules! pg_finfo_v1 {
    ($name:ident, $finfo:ident) => {
        static $name: pgrx::pg_sys::Pg_finfo_record = pgrx::pg_sys::Pg_finfo_record { api_version: 1 };

        #[unsafe(no_mangle)]
        pub extern "C" fn $finfo() -> *const pgrx::pg_sys::Pg_finfo_record {
            &raw const $name
        }
    };
}

pg_finfo_v1!(FINFO_TYPMOD_IN, pg_finfo_kmoney_typmod_in);
pg_finfo_v1!(FINFO_TYPMOD_OUT, pg_finfo_kmoney_typmod_out);
pg_finfo_v1!(FINFO_RECV, pg_finfo_kmoney_recv);
pg_finfo_v1!(FINFO_MIXED_RECV, pg_finfo_kmoney_mixed_recv);
