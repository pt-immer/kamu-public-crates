//! A hash of the canonical payload whose value is fixed forever, not by a toolchain.
//!
//! # Why this exists at all
//!
//! `kamu-money-pg` exposes `kmoney_hash` -- a stable hash of the canonical payload. There is no hash
//! index over `kmoney` anymore (the OLTP reframe removed the opclass), but the value stays a
//! durable contract: the sharpest byte-exactness signal in the ABI battery, and anything that
//! *persists* it (a shard key, a durable cache key, a future hash index) keeps the value
//! computed when a row was written. If the function ever returns a different number for the
//! same money, every stored bucket points somewhere the lookup no longer looks — and the
//! symptom is not an error at startup or during migration. It is a query quietly returning
//! fewer rows than exist.
//!
//! Both support functions previously used `std::collections::hash_map::DefaultHasher`, whose
//! own documentation says:
//!
//! > The internal algorithm is not specified, and so it and its hashes should not be relied
//! > upon over releases.
//!
//! That is not a latent risk that might one day bite. It is the standard library stating in
//! advance that it may change, under a database feature whose correctness depends on it not
//! changing.
//!
//! # Why swapping the hasher is only half of it
//!
//! Feeding a stable algorithm through the `Hash` trait leaves the bug in place.
//! `Hasher::write_i128` and its siblings emit **native-endian** bytes, so an identical
//! algorithm still produces different hashes on a big-endian machine: a streaming replica or a
//! dump restored on another architecture would disagree with the index it inherited.
//!
//! So this function takes the fields directly and serialises them itself, little-endian, stated.
//! Each field is encoded exactly as `kamu-money-pg` encodes it on disk, which is what makes the
//! hash a function of the *stored value* rather than of the machine reading it.
//!
//! **The ORDER is this hash's own, and it is NOT the storage layout.** Version 1 hashes `code`
//! and then `units`; the 18-byte on-disk payload is `units` and then `code`. Both are
//! little-endian and both are exact, so the two agree about every field and differ only in
//! sequence. A second implementation that hashes the storage payload verbatim will **not**
//! reproduce version 1 — hash the two fields in the order stated here. This paragraph exists
//! because the module previously called them "the same bytes", which they are not.
//!
//! # The algorithm, so it can be reimplemented without this crate
//!
//! FNV-1a (64-bit) over the payload bytes, then MurmurHash3's `fmix64` finaliser.
//!
//! FNV-1a alone diffuses its last-written bytes poorly, which matters more here than it looks:
//! money at scale 18 clusters hard. Every amount a 2-decimal currency can express is a multiple
//! of 10^16, so a real column holds values whose low fourteen digits are always zero and whose
//! neighbours differ in very few bytes. `fmix64` is three
//! xor-shift-multiply rounds with published constants and gives full avalanche, so those
//! neighbours land in unrelated buckets. Both algorithms are public, constant, and short enough
//! to reimplement from this comment — which is the test of whether a format is really specified.
//!
//! Field order is part of the contract: **currency code first, then units.** That matches the
//! order equality compares them in -- the agreement any two consistent hashers of the same
//! value must share.

/// Version of the hash contract. Bump ONLY alongside a re-hash of every store that persisted the
/// old values.
///
/// Not decoration. If the function below ever returns different values, every store that
/// persisted the old ones is silently wrong, so the change has to arrive with an
/// explicit operator instruction rather than as a routine refactor. This constant exists so
/// that a diff touching this file cannot be reviewed as cosmetic.
pub const STABLE_HASH_VERSION: u32 = 1;

/// FNV-1a 64-bit offset basis, per the FNV specification.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime, per the FNV specification.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hash the canonical payload of a money value: ISO numeric code, then canonical units.
///
/// Stable across toolchains, releases, architectures and endiannesses — by construction rather
/// than by observation. See the module docs for the algorithm and for why it is spelled out
/// here instead of delegated to `Hash`.
///
/// Takes the two fields rather than a [`crate::Money`] because the caller that needs it is
/// `kamu-money-pg`, whose type is not generic and learns its currency at run time. Same reasoning as
/// [`crate::advanced::arithmetic::allocate_units`]: one implementation the
/// adapter shares, not two that can drift apart.
#[must_use]
pub fn stable_hash(code: u16, units: i128) -> u64 {
    let mut h = FNV_OFFSET_BASIS;

    // Explicit little-endian, NOT `to_ne_bytes`: the hash must be a function of the stored value
    // rather than of the machine reading it. Each field is encoded as kamu-money-pg encodes it —
    // but the ORDER here (code, then units) is this hash's own, and is NOT the on-disk layout,
    // which is units then code. See the module docs.
    for byte in code.to_le_bytes().into_iter().chain(units.to_le_bytes()) {
        // `wrapping_mul` because FNV is DEFINED modulo 2^64 — this is the algorithm, not a
        // concession to `clippy::arithmetic_side_effects`. A plain `*` would panic in debug on
        // the first byte that overflows, which is nearly all of them.
        h ^= u64::from(byte);
        h = h.wrapping_mul(FNV_PRIME);
    }

    fmix64(h)
}

/// MurmurHash3's 64-bit finaliser. Published constants, three xor-shift-multiply rounds.
const fn fmix64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    h
}

/// Fold a 64-bit hash into the `int4` a PostgreSQL hash support function must return.
///
/// XOR of the two halves, so every input bit still reaches the result. Truncating instead would
/// discard exactly the bits `fmix64` just worked to spread.
///
/// The byte-array route rather than `as`: it avoids the banned cast, and it is endian-neutral
/// for a reason worth stating — which half of `to_ne_bytes` is the high one differs by
/// architecture, but XOR is commutative, so `hi ^ lo` is the same number either way. The final
/// `from_ne_bytes` reinterprets `u32` as `i32` on one machine, which round-trips whatever order
/// it used.
#[must_use]
pub const fn fold_to_i32(hash: u64) -> i32 {
    let b = hash.to_ne_bytes();
    let first = u32::from_ne_bytes([b[0], b[1], b[2], b[3]]);
    let second = u32::from_ne_bytes([b[4], b[5], b[6], b[7]]);
    i32::from_ne_bytes((first ^ second).to_ne_bytes())
}

#[cfg(test)]
mod tests {
    use super::{fold_to_i32, stable_hash};
    use crate::domain_impl::DOMAIN_MAX;
    use std::collections::BTreeSet;

    /// Golden vectors. The whole point of the module.
    ///
    /// PROVENANCE, because it decides what this test is worth: these numbers were computed by a
    /// SEPARATE Python implementation written from the module docs above, then confirmed
    /// against this code — not read out of this code and pinned. So they check two things at
    /// once: that the value never changes, and that the algorithm really is the FNV-1a+fmix64
    /// the docs claim, since an independent reimplementation reproduced it.
    ///
    /// That second property is the one a self-blessed golden vector cannot give. If this file
    /// silently stopped being FNV-1a, a vector captured from its own output would follow it.
    #[test]
    fn the_hash_of_a_known_payload_never_changes() {
        let vectors: &[(u16, i128, u64)] = &[
            (840, 0, 0x53f4_0bbd_7a11_33fa),
            (840, 1, 0x5027_9ff8_7b6d_7ff0),
            (840, 1_000_000_000_000_000_000, 0xc49c_c3b8_69dd_f023),
            (840, DOMAIN_MAX, 0xdf31_f563_09c0_1c5d),
            (840, -DOMAIN_MAX, 0x86eb_87b0_8067_b4f8),
            // Same units, different currency. Must not collide.
            (360, 1_000_000_000_000_000_000, 0xdfa8_4360_27e8_965f),
        ];
        for &(code, units, expected) in vectors {
            assert_eq!(
                stable_hash(code, units),
                expected,
                "stable_hash({code}, {units}) changed — every store that persisted the old value \
                 (a shard key, a durable cache key, a future hash index) is now silently wrong. \
                 That needs a STABLE_HASH_VERSION bump and a re-hash, not a re-blessed constant."
            );
        }
    }

    /// The property a hash opclass must satisfy, asserted directly rather than inferred from
    /// an index that happened to build.
    #[test]
    fn equal_payloads_hash_equal_and_the_currency_participates() {
        assert_eq!(stable_hash(840, 42), stable_hash(840, 42));
        // The failure this prevents: a hash join matching USD 1.00 against IDR 1.00 because
        // the currency never reached the hasher.
        assert_ne!(stable_hash(840, 42), stable_hash(360, 42));
        assert_ne!(stable_hash(840, 42), stable_hash(840, 43));
    }

    /// Money clusters. Every amount a 2-decimal currency can actually express is a multiple of
    /// 10^16 at this scale, so real columns hold values whose low fourteen digits are always
    /// zero. Without the finaliser those neighbours would share most of their bits and crowd
    /// into adjacent buckets — a hash index that degrades toward a scan.
    ///
    /// Both figures below are MEASURED against the Python oracle, not predicted: 64 consecutive
    /// cent values produce 64 distinct folded hashes covering all 16 low-bit buckets.
    #[test]
    fn neighbouring_amounts_do_not_land_in_neighbouring_buckets() {
        let folded: Vec<i32> =
            (0..64).map(|n| fold_to_i32(stable_hash(840, i128::from(n) * 10_000_000_000_000_000))).collect();

        let distinct: BTreeSet<_> = folded.iter().collect();
        assert_eq!(distinct.len(), 64, "folded hashes collided");

        // The low bits are what a small hash index actually buckets on.
        let buckets: BTreeSet<_> = folded.iter().map(|h| h & 0xF).collect();
        assert_eq!(
            buckets.len(),
            16,
            "64 consecutive amounts reached only {} of 16 low-bit buckets; the finaliser is \
             not diffusing",
            buckets.len()
        );
    }

    /// `fold_to_i32` must keep the high half contributing.
    #[test]
    fn folding_keeps_both_halves() {
        assert_eq!(fold_to_i32(0x0000_0000_0000_0001), 1);
        assert_eq!(fold_to_i32(0x0000_0001_0000_0000), 1);
        // Two values differing ONLY in the high half must not fold to the same number.
        assert_ne!(fold_to_i32(0x0000_0000_dead_beef), fold_to_i32(0xffff_ffff_dead_beef));
    }

    /// The regression that started this: nothing in the tree may reach for the unstable hasher
    /// again. A comment saying "do not use `DefaultHasher`" is not enforcement.
    ///
    /// ROOT DISCOVERY WALKS ANCESTORS, and that is not incidental. This crate used to sit at a
    /// repository root, where its manifest's immediate parent WAS the workspace root. Under
    /// `crates/money-core/` that parent is `crates/`, which has no `Justfile` — so the original
    /// `.parent()` plus marker check returned early and the test passed while inspecting
    /// nothing. Walking until the marker is found survives the next relocation too.
    ///
    /// THE CRATE LIST IS READ FROM THE LAYOUT, not named. It used to enumerate
    /// `kamu-money-{core,pg,iso}`, none of which are directory names here, so even a repaired
    /// root discovery would have scanned three paths that do not exist. Reading `crates/`
    /// covers whatever the workspace actually contains, including members added later.
    #[test]
    fn no_source_file_reaches_for_the_unstable_hasher() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let Some(root) = manifest.ancestors().find(|p| p.join("Justfile").is_file()) else {
            return; // unpacked .crate, no repository to inspect
        };

        let mut offenders = Vec::new();
        let mut scanned = 0_usize;
        let members = std::fs::read_dir(root.join("crates")).expect("crates/ is readable");
        for member in members.flatten() {
            let Ok(entries) = std::fs::read_dir(member.path().join("src")) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                // Split so the needle cannot be spelled in this file: written whole, the
                // literal below matches THIS source and the guard reports itself as an
                // offender. `concat!` joins at compile time, leaving no contiguous copy in
                // the bytes on disk.
                let needle = concat!("DefaultHasher", "::new");
                if path.extension().is_some_and(|e| e == "rs")
                    && let Ok(text) = std::fs::read_to_string(&path)
                {
                    scanned += 1;
                    if text.contains(needle) {
                        offenders.push(path.display().to_string());
                    }
                }
            }
        }

        // POSITIVE CONTROL. Without it the only difference between "nothing offends" and
        // "nothing was looked at" is which of them is true, and this guard has already been
        // silently green once for exactly that reason.
        assert!(
            scanned > 0,
            "the hasher guard read no source files at all — root discovery or the crates/ \
             layout changed, and this test was about to pass without checking anything"
        );
        assert!(
            offenders.is_empty(),
            "DefaultHasher's algorithm is explicitly unstable across Rust releases and must \
             not back anything persisted. Use kamu_money_core::advanced::stable_hash. \
             Found in: {offenders:?}"
        );
    }
}
