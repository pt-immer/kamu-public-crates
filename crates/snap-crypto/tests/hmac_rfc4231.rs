//! RFC 4231 HMAC-SHA512 known-answer tests.
//!
//! Locks the on-wire HMAC output byte-exact so any future refactor of
//! [`kamu_snap_crypto::HmacSigner`] cannot silently change the algorithm.

use kamu_snap_crypto::HmacSigner;

fn check(secret: &[u8], data: &[u8], expected_hex: &str) {
    let signer = HmacSigner::new(secret).expect("HMAC init");
    let sig = signer.sign(data);
    assert_eq!(sig.to_hex_lower(), expected_hex);
    // Round-trip verify against the same payload.
    signer.verify(&sig, data).expect("verify same payload");
}

#[test]
fn rfc4231_case_1() {
    // Key = 0x0b * 20, Data = "Hi There"
    let key = [0x0bu8; 20];
    check(
        &key,
        b"Hi There",
        "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854",
    );
}

#[test]
fn rfc4231_case_2() {
    check(
        b"Jefe",
        b"what do ya want for nothing?",
        "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737",
    );
}

#[test]
fn rfc4231_case_3() {
    let key = [0xaau8; 20];
    let data = [0xddu8; 50];
    check(
        &key,
        &data,
        "fa73b0089d56a284efb0f0756c890be9b1b5dbdd8ee81a3655f83e33b2279d39bf3e848279a722c806b485a47e67c807b946a337bee8942674278859e13292fb",
    );
}

#[test]
fn rfc4231_case_4() {
    let key: Vec<u8> = (1u8..=25).collect();
    let data = [0xcdu8; 50];
    check(
        &key,
        &data,
        "b0ba465637458c6990e5a8c5f61d4af7e576d97ff94b872de76f8050361ee3dba91ca5c11aa25eb4d679275cc5788063a5f19741120c4f2de2adebeb10a298dd",
    );
}

#[test]
fn rfc4231_case_6_large_key() {
    let key = [0xaau8; 131];
    check(
        &key,
        b"Test Using Larger Than Block-Size Key - Hash Key First",
        "80b24263c7c1a3ebb71493c1dd7be8b49b46d1f41b4aeec1121b013783f8f3526b56d037e05f2598bd0fd2215d6a1e5295e64f73f63f0aec8b915a985d786598",
    );
}

#[test]
fn rfc4231_case_7_large_key_and_data() {
    let key = [0xaau8; 131];
    check(
        &key,
        b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.",
        "e37b6a775dc87dbaa4dfa9f96e5e3ffddebd71f8867289865df5a32d20cdc944b6022cac3c4982b10d5eeb55c3e4de15134676fb6de0446065c97440fa8c6a58",
    );
}

#[test]
fn verify_rejects_wrong_signature() {
    let signer = HmacSigner::new(b"secret").unwrap();
    let sig = signer.sign(b"payload-a");
    let result = signer.verify(&sig, b"payload-b");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::SymmetricVerifyFailed)));
}

#[test]
fn verify_rejects_wrong_secret() {
    let signer_a = HmacSigner::new(b"secret-a").unwrap();
    let signer_b = HmacSigner::new(b"secret-b").unwrap();
    let sig = signer_a.sign(b"payload");
    let result = signer_b.verify(&sig, b"payload");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::SymmetricVerifyFailed)));
}

#[test]
fn signer_is_shareable_across_calls() {
    let signer = HmacSigner::new(b"secret").unwrap();
    let sig_1 = signer.sign(b"first");
    let sig_2 = signer.sign(b"second");
    assert_ne!(sig_1, sig_2);
    signer.verify(&sig_1, b"first").unwrap();
    signer.verify(&sig_2, b"second").unwrap();
}
