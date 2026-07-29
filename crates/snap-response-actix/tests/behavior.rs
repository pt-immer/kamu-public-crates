use std::cell::Cell;

use actix_web::{Responder, body::to_bytes, http::header, test::TestRequest};
use kamu_snap_response::{Error, PayloadError, ServiceCode, SnapResponse};
use kamu_snap_response_actix::SnapResponderExt;
use serde::{
    Deserialize, Serialize,
    ser::{Error as _, SerializeStruct as _},
};

#[derive(Debug, Deserialize, Serialize)]
struct Payload {
    value: String,
}

fn service() -> ServiceCode {
    ServiceCode::try_from(11).unwrap()
}

fn render<T: Serialize>(response: SnapResponse<T>) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
    response.into_actix().respond_to(&TestRequest::default().to_http_request())
}

fn read_body(response: actix_web::HttpResponse<actix_web::body::BoxBody>) -> actix_web::web::Bytes {
    actix_web::rt::System::new().block_on(to_bytes(response.into_body())).unwrap()
}

#[test]
fn valid_success_and_failure_keep_status_json_and_body_in_sync() {
    let cases = [
        (SnapResponse::success(Payload { value: "ok".into() }, service()).unwrap(), 200, "2001100"),
        (SnapResponse::failure(Error::InsufficientFunds, service()), 403, "4031114"),
    ];

    for (response, expected_status, expected_code) in cases {
        let response = render(response);
        assert_eq!(response.status().as_u16(), expected_status);
        assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
        let body: serde_json::Value = serde_json::from_slice(&read_body(response)).unwrap();
        assert_eq!(body["responseCode"], expected_code);
    }
}

#[test]
fn malformed_service_digits_force_500_but_preserve_body() {
    let response: SnapResponse<Payload> =
        serde_json::from_str(r#"{"responseCode":"200ab00","responseMessage":"provider defect"}"#).unwrap();
    let response = render(response);

    assert_eq!(response.status().as_u16(), 500);
    let body: serde_json::Value = serde_json::from_slice(&read_body(response)).unwrap();
    assert_eq!(body["responseCode"], "200ab00");
}

#[derive(Debug, Serialize)]
struct Collision {
    #[serde(rename = "responseCode")]
    shadow: &'static str,
}

#[test]
fn reserved_payload_key_never_reaches_the_adapter() {
    let error = SnapResponse::success(Collision { shadow: "5000000" }, service()).unwrap_err();
    assert!(matches!(error, PayloadError::ReservedKey { key: "responseCode" }));
}

#[derive(Debug)]
struct FailAfterValidation(Cell<bool>);

impl Serialize for FailAfterValidation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.replace(true) {
            return Err(S::Error::custom("intentional second-pass failure"));
        }
        let mut object = serializer.serialize_struct("Payload", 1)?;
        object.serialize_field("value", "ok")?;
        object.end()
    }
}

#[test]
fn serialization_failure_cannot_retain_a_success_status() {
    let response = SnapResponse::success(FailAfterValidation(Cell::new(false)), service()).unwrap();
    let response = render(response);

    assert_eq!(response.status().as_u16(), 500);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
    let body: serde_json::Value = serde_json::from_slice(&read_body(response)).unwrap();
    assert_eq!(body["responseCode"], "5001101");
    assert_eq!(body["responseMessage"], "Internal Server Error");
}
