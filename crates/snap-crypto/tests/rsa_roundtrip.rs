//! RSA-SHA256 sign + verify tests.

use kamu_snap_crypto::{RsaSigner, RsaVerifier, Signature};
use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

// SNAP BI requires RSA 2048.
const BITS: usize = 2048;
const PAYLOAD: &[u8] = b"the quick brown fox jumps over the lazy dog";
const OPENSSL_PAYLOAD: &[u8] = b"SNAP BI RSA-SHA256 interoperability vector";
const OPENSSL_PUBLIC_KEY: &str = r"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEArWVCA/f6g9MQIKc9VUev
iVG9ozPRYe+NBYPnn6w7uJx9PmrkVtTFLLffmBXhOBdeiopU78z5KiSkP/eRHNo1
lBur8NDoaiewkuCY0DXJCtxBZ8cOQHS86KUVw7riNz7NT7+aUtdoct/72b+J5SES
YlTSGhPbIoDaAXax9lYiXVwemqOp7f7RzZl+G0+z5zhmiO8KRyK8gjOJA3nT14Ie
3aEVBeRDvKo0VmVUee21KAanBXGLBzI+wblWX3rwaRnFxy7jQK6DeFQHYfA1LE0e
LA2BifWZ208q6ynniy+frErV+nhqztWOe1XINlnK4GCdefC2z5Eg43Rp1A02lGvw
VQIDAQAB
-----END PUBLIC KEY-----";
const OPENSSL_SIGNATURE: &str = "HxPQj5qHU9HIDStkmxP98RF2mKNS2+wPdxmjVZVOKKgordXjkZfhIADOtdvJqZOowYCkIasdJ2emx2vqgTcyWWuDk9+DUgWW3mYPJ3x0aCwbyqjmkZlzPzttaqWkzYLW+IENme3UsqGXB+J4d0o6p2MEFnZbowc4PFy+iNWEqDL7QvZbp7NfK2VQnjMRm0MXqReTmvoySCOo/6ur90Zm5jUX0qEKtVh4c3M7yLgpk7I8LECiSBeVC+9JGKRwq0cnouEWVVI4F/H5LC33XMjBPvM5EJdKLTkYQNr4UPNhvZih9zAZy7iv0+qac739s+6RSDW627UKtE6u1zNzRmK4mQ==";

fn ephemeral_pair() -> (String, String) {
    let mut rng = rand_core::OsRng;
    let priv_key = RsaPrivateKey::new(&mut rng, BITS).expect("rsa keygen");
    let pub_key = priv_key.to_public_key();
    let priv_pem = priv_key.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    let pub_pem = pub_key.to_public_key_pem(LineEnding::LF).unwrap();
    (priv_pem, pub_pem)
}

fn round_trip(priv_pem: &str, pub_pem: &str) {
    let signer = RsaSigner::from_pkcs8_pem(priv_pem).unwrap();
    let verifier = RsaVerifier::from_spki_pem(pub_pem).unwrap();
    let sig = signer.sign(PAYLOAD);
    verifier.verify(&sig, PAYLOAD).unwrap();
}

#[test]
fn pkcs1v15_sha256_round_trip() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    round_trip(&priv_pem, &pub_pem);
}

#[test]
fn verifies_openssl_known_answer() {
    // Generated independently with:
    // openssl dgst -sha256 -sign private.pem message
    let verifier = RsaVerifier::from_spki_pem(OPENSSL_PUBLIC_KEY).unwrap();
    let signature = Signature::from_base64(OPENSSL_SIGNATURE).unwrap();
    verifier.verify(&signature, OPENSSL_PAYLOAD).unwrap();
}

#[test]
fn verify_rejects_wrong_key() {
    let (priv_a, _pub_a) = ephemeral_pair();
    let (_priv_b, pub_b) = ephemeral_pair();
    let signer = RsaSigner::from_pkcs8_pem(&priv_a).unwrap();
    let verifier = RsaVerifier::from_spki_pem(&pub_b).unwrap();
    let sig = signer.sign(PAYLOAD);
    assert!(matches!(verifier.verify(&sig, PAYLOAD), Err(kamu_snap_crypto::Error::AsymmetricVerifyFailed)));
}

#[test]
fn verify_rejects_wrong_payload() {
    let (priv_pem, pub_pem) = ephemeral_pair();
    let signer = RsaSigner::from_pkcs8_pem(&priv_pem).unwrap();
    let verifier = RsaVerifier::from_spki_pem(&pub_pem).unwrap();
    let sig = signer.sign(PAYLOAD);
    assert!(matches!(
        verifier.verify(&sig, b"tampered"),
        Err(kamu_snap_crypto::Error::AsymmetricVerifyFailed)
    ));
}

#[test]
fn rejects_garbage_private_pem() {
    let result = RsaSigner::from_pkcs8_pem("not a PEM");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::InvalidSecretKey(_))));
}

#[test]
fn rejects_garbage_public_pem() {
    let result = RsaVerifier::from_spki_pem("not a PEM");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::InvalidPublicKey(_))));
}

#[test]
fn rejects_wrong_raw_signature_length_without_claiming_base64_failure() {
    let (_, public_pem) = ephemeral_pair();
    let verifier = RsaVerifier::from_spki_pem(&public_pem).unwrap();
    let result = verifier.verify(&kamu_snap_crypto::Signature::from_bytes([0_u8; 17]), PAYLOAD);
    assert!(matches!(result, Err(kamu_snap_crypto::Error::InvalidRawSignature(_))));
}
