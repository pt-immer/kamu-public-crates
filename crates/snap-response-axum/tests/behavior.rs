use std::cell::Cell;

use axum::{
    body::to_bytes,
    http::{StatusCode, header},
    response::IntoResponse,
};
use kamu_snap_response::{Error, PayloadError, ServiceCode, SnapResponse};
use kamu_snap_response_axum::SnapResponderExt;
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

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn valid_success_and_failure_keep_status_json_and_body_in_sync() {
    let cases = [
        (
            SnapResponse::success(Payload { value: "ok".into() }, service()).unwrap(),
            StatusCode::OK,
            "2001100",
        ),
        (SnapResponse::failure(Error::InsufficientFunds, service()), StatusCode::FORBIDDEN, "4031114"),
    ];

    for (response, expected_status, expected_code) in cases {
        let response = response.into_axum().into_response();
        assert_eq!(response.status(), expected_status);
        assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
        assert_eq!(body_json(response).await["responseCode"], expected_code);
    }
}

#[tokio::test]
async fn malformed_service_digits_force_500_but_preserve_body() {
    let response: SnapResponse<Payload> =
        serde_json::from_str(r#"{"responseCode":"200ab00","responseMessage":"provider defect"}"#).unwrap();
    let response = response.into_axum().into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body_json(response).await["responseCode"], "200ab00");
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

#[tokio::test]
async fn serialization_failure_cannot_retain_a_success_status() {
    let response = SnapResponse::success(FailAfterValidation(Cell::new(false)), service()).unwrap();
    let response = response.into_axum().into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
    let body = body_json(response).await;
    assert_eq!(body["responseCode"], "5001101");
    assert_eq!(body["responseMessage"], "Internal Server Error");
}
