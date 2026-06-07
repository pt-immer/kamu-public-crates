//! Round-trip serialisation tests for [`SnapResponse<T>`] + the F-08 +
//! F-03 regression checks.

use kamu_snap_response::{Error, ServiceCode, SnapResponse};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BalancePayload {
    #[serde(rename = "accountNo")]
    account_no: String,
    #[serde(rename = "currentBalance")]
    current_balance: String,
}

#[test]
fn ok_response_serialize_round_trip() {
    let payload = BalancePayload { account_no: "1234567890".into(), current_balance: "1000000.00".into() };
    let svc = ServiceCode::new(11).unwrap();
    let resp = SnapResponse::ok(payload.clone(), svc, 0);

    let wire = serde_json::to_string(&resp).unwrap();
    // Wire shape: envelope fields + payload fields at the SAME level.
    assert!(wire.contains("\"responseCode\":\"2001100\""));
    assert!(wire.contains("\"responseMessage\":\"Successful\""));
    assert!(wire.contains("\"accountNo\":\"1234567890\""));

    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(&wire).unwrap();
    assert_eq!(parsed.envelope.response_code.raw(), "2001100");
    assert_eq!(parsed.envelope.response_message, "Successful");
    assert_eq!(parsed.payload, Some(payload));
}

#[test]
fn err_response_serialize_round_trip() {
    let svc = ServiceCode::new(11).unwrap();
    let resp = SnapResponse::<BalancePayload>::err(Error::Unauthorized("invalid token".into()), svc);

    let wire = serde_json::to_string(&resp).unwrap();
    assert!(wire.contains("\"responseCode\":\"4011100\""));
    assert!(wire.contains("Unauthorized"));

    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(&wire).unwrap();
    assert_eq!(parsed.envelope.response_code.raw(), "4011100");
    assert!(parsed.payload.is_none());
}

#[test]
fn payload_schema_mismatch_fails_loudly() {
    // F-08 fix: payload deserialisation errors must propagate, not silently
    // become payload = None.
    //
    // Wire has both envelope fields and a payload-shaped object, but a field
    // is missing — should be an error, not a silent None.
    let wire = r#"{
        "responseCode": "2001100",
        "responseMessage": "Successful",
        "accountNo": "1234567890"
    }"#;
    let result: Result<SnapResponse<BalancePayload>, _> = serde_json::from_str(wire);
    assert!(result.is_err(), "missing required payload field must error, got: {result:?}");
}

#[test]
fn error_response_without_payload_deserializes() {
    // An error response with no payload fields must produce payload = None for
    // any T (otherwise client side cannot decode error envelopes generically).
    let wire = r#"{"responseCode":"4011100","responseMessage":"Unauthorized."}"#;
    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(wire).unwrap();
    assert!(parsed.payload.is_none());
    assert_eq!(parsed.envelope.response_code.raw(), "4011100");
}

#[test]
fn malformed_response_code_still_round_trips() {
    // F-03 fix: a malformed responseCode must not blow up deserialisation.
    let wire = r#"{"responseCode":"500000","responseMessage":"General Error"}"#;
    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(wire).unwrap();
    assert_eq!(parsed.envelope.response_code.raw(), "500000");
    assert_eq!(parsed.envelope.response_message, "General Error");
    // Defensive parser exposes None for derived fields.
    assert!(parsed.envelope.response_code.http().is_none());
}

#[test]
fn ok_with_unit_payload_serialises_envelope_only() {
    let svc = ServiceCode::new(11).unwrap();
    let resp = SnapResponse::ok((), svc, 0);
    let wire = serde_json::to_string(&resp).unwrap();
    assert!(wire.contains("\"responseCode\":\"2001100\""));
}

#[test]
fn missing_response_code_fails() {
    let wire = r#"{"responseMessage":"x"}"#;
    let result: Result<SnapResponse<BalancePayload>, _> = serde_json::from_str(wire);
    assert!(result.is_err());
}
