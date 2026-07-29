//! Stable hash of the canonical money payload.
//!
//! Version 1 applies 64-bit FNV-1a, then MurmurHash3 `fmix64`, to little-endian fields in this
//! order: ISO numeric `code`, then canonical `units`. This differs from `kamu-money-pg` storage
//! order (`units`, then `code`). Field order, byte order, constants, and finalizer are durable:
//! persisted shard or cache keys require a version bump and re-hash if any changes.
//!
//! `DefaultHasher` and integer [`core::hash::Hash`] encoding are unsuitable because their
//! algorithm or native-endian input is not a stable storage contract.

/// Hash contract version. Changing hash output requires a version bump and re-hash of persisted
/// values.
pub const STABLE_HASH_VERSION: u32 = 1;

/// FNV-1a 64-bit offset basis, per the FNV specification.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime, per the FNV specification.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hash the canonical payload of a money value: ISO numeric code, then canonical units.
///
/// Stable across toolchains, releases, architectures, and endiannesses.
///
/// Takes the two fields rather than a [`crate::Money`] because the caller that needs it is
/// `kamu-money-pg`, whose type is not generic and learns its currency at run time. Same reasoning as
/// [`crate::advanced::arithmetic::allocate_units`]: one implementation the
/// adapter shares, not two that can drift apart.
#[must_use]
pub fn stable_hash(code: u16, units: i128) -> u64 {
    let mut h = FNV_OFFSET_BASIS;

    // Hash order is code then units; storage order is units then code.
    for byte in code.to_le_bytes().into_iter().chain(units.to_le_bytes()) {
        // FNV multiplication is defined modulo 2^64.
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
/// XORs both halves so every input bit contributes. XOR commutativity makes the native-byte
/// implementation endian-neutral.
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

    /// Golden vectors computed independently from the documented algorithm.
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

    /// Consecutive cent values remain distinct and cover every four-bit bucket.
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
        // A high-half difference must affect the result.
        assert_ne!(fold_to_i32(0x0000_0000_dead_beef), fold_to_i32(0xffff_ffff_dead_beef));
    }
}
