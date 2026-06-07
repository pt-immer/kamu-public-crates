//! RSA sign + verify round-trip for every shipped [`SignatureScheme`].
//!
//! Generates an ephemeral 1024-bit keypair per scheme. Bit size kept low to
//! keep test runtime reasonable; production keys must be 2048+ bits.

use kamu_snap_crypto::rsa::{Pkcs1v15Sha256, Pkcs1v15Sha512, PssSha256, PssSha512, SignatureScheme};
use kamu_snap_crypto::{RsaSigner, RsaVerifier};
use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

// 2048 bits: PSS-SHA512 requires `key_bits >= 2 * hash_bits + 16`, so 1024 is
// too small for that scheme. 2048 covers all four shipped schemes; total test
// time is a few seconds.
const BITS: usize = 2048;
const PAYLOAD: &[u8] = b"the quick brown fox jumps over the lazy dog";

fn ephemeral_pair() -> (String, String) {
    let mut rng = rand_core::OsRng;
    let priv_key = RsaPrivateKey::new(&mut rng, BITS).expect("rsa keygen");
    let pub_key = priv_key.to_public_key();
    let priv_pem = priv_key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    let pub_pem = pub_key.to_public_key_pem(LineEnding::LF).unwrap();
    (priv_pem, pub_pem)
}

fn round_trip<S: SignatureScheme>(priv_pem: &str, pub_pem: &str) {
    let signer = RsaSigner::<S>::from_pkcs8_pem(priv_pem).unwrap();
    let verifier = RsaVerifier::<S>::from_pkcs8_public_pem(pub_pem).unwrap();
    let sig = signer.sign(PAYLOAD);
    verifier.verify(&sig, PAYLOAD).unwrap();
}

#[test]
fn pkcs1v15_sha256_round_trip() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    round_trip::<Pkcs1v15Sha256>(&priv_pem, &pub_pem);
}

#[test]
fn pkcs1v15_sha512_round_trip() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    round_trip::<Pkcs1v15Sha512>(&priv_pem, &pub_pem);
}

#[test]
fn pss_sha256_round_trip() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    round_trip::<PssSha256>(&priv_pem, &pub_pem);
}

#[test]
fn pss_sha512_round_trip() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    round_trip::<PssSha512>(&priv_pem, &pub_pem);
}

#[test]
fn verify_rejects_wrong_key() {
    let (priv_a, _pub_a) = ephemeral_pair();
    let (_priv_b, pub_b) = ephemeral_pair();
    let signer = RsaSigner::<Pkcs1v15Sha256>::from_pkcs8_pem(&priv_a).unwrap();
    let verifier = RsaVerifier::<Pkcs1v15Sha256>::from_pkcs8_public_pem(&pub_b).unwrap();
    let sig = signer.sign(PAYLOAD);
    assert!(matches!(verifier.verify(&sig, PAYLOAD), Err(kamu_snap_crypto::Error::AsymmetricVerifyFailed)));
}

#[test]
fn verify_rejects_wrong_payload() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    let signer = RsaSigner::<Pkcs1v15Sha256>::from_pkcs8_pem(&priv_pem).unwrap();
    let verifier = RsaVerifier::<Pkcs1v15Sha256>::from_pkcs8_public_pem(&pub_pem).unwrap();
    let sig = signer.sign(PAYLOAD);
    assert!(matches!(
        verifier.verify(&sig, b"tampered"),
        Err(kamu_snap_crypto::Error::AsymmetricVerifyFailed)
    ));
}

#[test]
fn rejects_garbage_private_pem() {
    let result = RsaSigner::<Pkcs1v15Sha256>::from_pkcs8_pem("not a PEM");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::InvalidSecretKey(_))));
}

#[test]
fn rejects_garbage_public_pem() {
    let result = RsaVerifier::<Pkcs1v15Sha256>::from_pkcs8_public_pem("not a PEM");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::InvalidPublicKey(_))));
}
