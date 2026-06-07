//! Round-trip tests for the [`Signature`] newtype + [`Encoding`] enum.

use kamu_snap_crypto::{Encoding, Signature};

const PAYLOAD: &[u8] = b"\x00\x01\x02\xff\xfe\xfd hello SNAP BI";

#[test]
fn base64_round_trip() {
    let sig = Signature::from_bytes(PAYLOAD);
    let encoded = sig.to_base64();
    let decoded = Signature::from_base64(&encoded).unwrap();
    assert_eq!(decoded, sig);
}

#[test]
fn base64_url_nopad_round_trip() {
    let sig = Signature::from_bytes(PAYLOAD);
    let encoded = sig.to_base64_url_nopad();
    let decoded = Signature::from_base64_url_nopad(&encoded).unwrap();
    assert_eq!(decoded, sig);
}

#[test]
fn hex_round_trip() {
    let sig = Signature::from_bytes(PAYLOAD);
    let encoded = sig.to_hex_lower();
    let decoded = Signature::from_hex(&encoded).unwrap();
    assert_eq!(decoded, sig);
}

#[test]
fn dispatcher_dispatches_correctly() {
    let sig = Signature::from_bytes(PAYLOAD);
    for enc in [Encoding::Base64, Encoding::Base64UrlNoPad, Encoding::HexLower] {
        let encoded = sig.encode(enc);
        let decoded = Signature::decode(&encoded, enc).unwrap();
        assert_eq!(decoded, sig, "round-trip failed for {enc:?}");
    }
}

#[test]
fn malformed_base64_errors() {
    let result = Signature::from_base64("not-valid-base64!@#$");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::SignatureDecode { .. })));
}

#[test]
fn malformed_hex_errors() {
    let result = Signature::from_hex("zzz");
    assert!(matches!(result, Err(kamu_snap_crypto::Error::SignatureDecode { .. })));
}
