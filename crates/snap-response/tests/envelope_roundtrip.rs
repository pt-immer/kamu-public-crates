use kamu_snap_response::{Error, ErrorClass, PayloadError, ServiceCode, SnapResponse};
use serde::{
    Deserialize, Serialize,
    ser::{Error as _, SerializeStruct as _},
};
use std::cell::Cell;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalancePayload {
    account_no: String,
    current_balance: String,
}

fn service() -> ServiceCode {
    ServiceCode::try_from(11).unwrap()
}

#[test]
fn success_round_trip_preserves_typed_state() {
    let payload = BalancePayload { account_no: "1234567890".into(), current_balance: "1000000.00".into() };
    let response = SnapResponse::success(payload.clone(), service()).unwrap();

    let wire = serde_json::to_string(&response).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&wire).unwrap(),
        serde_json::json!({
            "responseCode": "2001100",
            "responseMessage": "Successful",
            "accountNo": "1234567890",
            "currentBalance": "1000000.00"
        })
    );

    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(&wire).unwrap();
    assert_eq!(parsed.http_status(), http::StatusCode::OK);
    assert_eq!(parsed.response_message(), "Successful");
    assert_eq!(parsed.payload(), Some(&payload));
    let SnapResponse::Success(success) = parsed else {
        panic!("expected success");
    };
    assert_eq!(success.response_code().as_str(), "2001100");
    assert_eq!(success.response_message(), "Successful");
    assert_eq!(success.into_payload(), payload);
}

#[test]
fn failure_round_trip_preserves_class_without_fake_context() {
    let response =
        SnapResponse::<BalancePayload>::failure(Error::Unauthorized("invalid token".into()), service());
    let wire = serde_json::to_string(&response).unwrap();
    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(&wire).unwrap();

    assert_eq!(parsed.response_code(), "4011100");
    assert_eq!(parsed.response_message(), "Unauthorized. invalid token");
    assert_eq!(parsed.error_class(), Some(ErrorClass::Unauthorized));
    assert!(parsed.payload().is_none());
    assert!(matches!(parsed, SnapResponse::Failure(_)));
}

#[test]
fn unit_payload_remains_a_success_state() {
    let response = SnapResponse::success((), service()).unwrap();
    let wire = serde_json::to_string(&response).unwrap();
    assert_eq!(wire, r#"{"responseCode":"2001100","responseMessage":"Successful"}"#);

    let parsed: SnapResponse<()> = serde_json::from_str(&wire).unwrap();
    assert!(matches!(parsed, SnapResponse::Success(_)));
    assert_eq!(parsed.payload(), Some(&()));
}

#[test]
fn malformed_code_is_explicit_and_forces_safe_http_status() {
    let wire = r#"{
        "responseCode": "200ab00",
        "responseMessage": "upstream defect",
        "diagnostic": "retained"
    }"#;
    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(wire).unwrap();

    assert_eq!(parsed.response_code(), "200ab00");
    assert_eq!(parsed.http_status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    assert!(parsed.valid_response_code().is_none());
    assert_eq!(parsed.raw_response_code().expect("malformed code").as_str(), "200ab00");
    let SnapResponse::Malformed(details) = &parsed else {
        panic!("expected malformed response");
    };
    assert_eq!(details.raw_payload().as_map().get("diagnostic"), Some(&serde_json::json!("retained")));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&parsed).unwrap()).unwrap(),
        serde_json::from_str::<serde_json::Value>(wire).unwrap()
    );
}

#[test]
fn success_payload_schema_mismatch_fails_loudly() {
    let wire = r#"{
        "responseCode": "2001100",
        "responseMessage": "Successful",
        "accountNo": "1234567890"
    }"#;
    let result = serde_json::from_str::<SnapResponse<BalancePayload>>(wire);

    assert!(result.is_err());
}

#[test]
fn failure_payload_is_preserved_without_decoding_as_success_data() {
    let wire = r#"{
        "responseCode": "4031114",
        "responseMessage": "Insufficient Funds",
        "providerReference": "ref-1"
    }"#;
    let parsed: SnapResponse<BalancePayload> = serde_json::from_str(wire).unwrap();
    let SnapResponse::Failure(details) = &parsed else {
        panic!("expected failure");
    };

    assert_eq!(details.error_class(), Some(ErrorClass::InsufficientFunds));
    assert_eq!(details.raw_payload().as_map().get("providerReference"), Some(&serde_json::json!("ref-1")));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&parsed).unwrap()).unwrap(),
        serde_json::from_str::<serde_json::Value>(wire).unwrap()
    );
}

#[derive(Debug, Serialize)]
struct ResponseCodeCollision {
    #[serde(rename = "responseCode")]
    value: String,
}

#[derive(Debug, Serialize)]
struct ResponseMessageCollision {
    #[serde(rename = "responseMessage")]
    value: String,
}

#[test]
fn reserved_payload_keys_are_rejected_before_response_construction() {
    let code_error =
        SnapResponse::success(ResponseCodeCollision { value: "shadow".into() }, service()).unwrap_err();
    assert!(matches!(code_error, PayloadError::ReservedKey { key: "responseCode" }));

    let message_error =
        SnapResponse::success(ResponseMessageCollision { value: "shadow".into() }, service()).unwrap_err();
    assert!(matches!(message_error, PayloadError::ReservedKey { key: "responseMessage" }));
}

#[derive(Serialize)]
struct NestedReservedName {
    details: serde_json::Value,
}

#[test]
fn nested_reserved_names_do_not_collide() {
    let response = SnapResponse::success(
        NestedReservedName { details: serde_json::json!({"responseCode": "nested"}) },
        service(),
    )
    .unwrap();
    let wire = serde_json::to_value(response).unwrap();

    assert_eq!(wire["details"]["responseCode"], "nested");
    assert_eq!(wire["responseCode"], "2001100");
}

#[test]
fn scalar_payload_is_rejected_at_the_boundary() {
    let error = SnapResponse::success("not a flat object", service()).unwrap_err();
    assert!(matches!(error, PayloadError::NotObject));
}

#[derive(Debug)]
struct AlwaysFails;

impl Serialize for AlwaysFails {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom("intentional failure"))
    }
}

#[test]
fn payload_serialization_failure_is_typed() {
    let error = SnapResponse::success(AlwaysFails, service()).unwrap_err();
    assert!(matches!(error, PayloadError::Serialization(_)));
    assert!(error.to_string().contains("intentional failure"));
}

#[derive(Debug)]
struct ChangesAfterValidation(Cell<bool>);

impl Serialize for ChangesAfterValidation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut object = serializer.serialize_struct("Payload", 1)?;
        if self.0.replace(true) {
            object.serialize_field("responseMessage", "shadow")?;
        } else {
            object.serialize_field("value", "valid")?;
        }
        object.end()
    }
}

#[test]
fn serialization_rechecks_reserved_keys_before_writing() {
    let response = SnapResponse::success(ChangesAfterValidation(Cell::new(false)), service()).unwrap();
    let error = serde_json::to_string(&response).unwrap_err();

    assert!(error.to_string().contains("payload key `responseMessage` is reserved"));
}

#[test]
fn explicit_success_case_and_payload_object_accessors_work() {
    let response = SnapResponse::success_with_case(
        BalancePayload { account_no: "1".into(), current_balance: "2".into() },
        service(),
        kamu_snap_response::CaseCode::try_from(7).unwrap(),
    )
    .unwrap();
    assert_eq!(response.response_code(), "2001107");

    let object = kamu_snap_response::PayloadObject::default();
    assert!(object.as_map().is_empty());
    assert!(object.into_map().is_empty());
}

#[test]
fn duplicate_envelope_keys_are_rejected_on_input() {
    let wire = r#"{
        "responseCode": "2001100",
        "responseCode": "5001100",
        "responseMessage": "ambiguous"
    }"#;
    assert!(serde_json::from_str::<SnapResponse<()>>(wire).is_err());
}

#[test]
fn unknown_valid_non_success_code_is_a_failure_without_known_class() {
    let wire = r#"{"responseCode":"4181142","responseMessage":"provider extension"}"#;
    let parsed: SnapResponse<()> = serde_json::from_str(wire).unwrap();

    assert_eq!(parsed.http_status(), http::StatusCode::IM_A_TEAPOT);
    assert_eq!(parsed.error_class(), None);
    assert!(matches!(parsed, SnapResponse::Failure(_)));
}
