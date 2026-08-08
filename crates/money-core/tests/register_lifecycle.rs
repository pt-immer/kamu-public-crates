//! The register's identity facts are frozen: persisted data depends on them.
//!
//! `kamu-money-pg` derives one SQL type per code, `kmoney_mixed` resolves its
//! stored 2-byte numeric against this register at every read, and
//! `stable_hash(code.numeric(), units)` is persisted by downstream systems. A
//! register refresh that removes a code orphans production columns; one that
//! changes a numeric silently reinterprets stored money and moves every
//! persisted hash without a `STABLE_HASH_VERSION` bump. The file-level SHA-256
//! gate makes any edition swap loud, but it cannot say *which* facts moved —
//! this digest pins exactly the facts persistence depends on.

use core::fmt::Write as _;

use kamu_money_core::Iso4217;

/// FNV-1a 64. Inline so the pin adds no dependency; the input is canonical
/// ASCII, so no hasher subtlety applies.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Every `(alpha3, numeric)` pair, digested in `EVERY` order.
///
/// This failing means a register update touched currency IDENTITY, not just
/// names or exponents. Before re-blessing the constant, apply the lifecycle
/// policy (`VENDORED.md`, "Identity facts are append-only"):
///
/// - an ADDED code is fine — re-bless, and note the new type in the extension's
///   release;
/// - a REMOVED code is a breaking release: existing `kmoney_<code>` columns and
///   stored `kmoney_mixed` rows of that currency become unreadable on upgrade.
///   The register keeps withdrawn codes instead;
/// - a CHANGED numeric is a data-corruption event: stored `kmoney_mixed`
///   payloads re-resolve to the wrong currency and every persisted
///   `stable_hash` moves. It requires a `STABLE_HASH_VERSION` decision, never a
///   silent re-bless.
#[test]
fn the_alpha3_to_numeric_mapping_is_frozen() {
    let mut canonical = String::new();
    for code in Iso4217::EVERY {
        writeln!(canonical, "{}:{}", code.alpha3(), code.numeric()).expect("writing to a String cannot fail");
    }
    assert_eq!(
        fnv1a64(canonical.as_bytes()),
        0x8AE9_E93E_B03A_E006,
        "the alpha3->numeric mapping changed; this is persisted-data identity, not a data \
         refresh -- read this test's doc comment before re-blessing"
    );
}
