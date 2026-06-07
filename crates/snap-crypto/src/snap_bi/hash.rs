//! Lowercase-hex SHA helpers used by the SNAP BI stringToSign formula.

use sha2::Digest;

/// SHA-256 of `bytes`, encoded as lowercase hex. Matches the canonical
/// body-hash form embedded in every SNAP BI service `stringToSign`.
pub fn sha256_lower_hex(bytes: impl AsRef<[u8]>) -> String {
    let digest = sha2::Sha256::digest(bytes.as_ref());
    hex::encode(digest)
}

/// SHA-512 of `bytes`, encoded as lowercase hex. Provided for variants that
/// mandate SHA-512 in the body-hash slot.
pub fn sha512_lower_hex(bytes: impl AsRef<[u8]>) -> String {
    let digest = sha2::Sha512::digest(bytes.as_ref());
    hex::encode(digest)
}
