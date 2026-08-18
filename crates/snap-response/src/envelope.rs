//! Domain responses and their flat SNAP BI wire representation.

use crate::{CaseCode, Error, ErrorClass, RawResponseCode, ResponseCode, ServiceCode, ValidResponseCode};
use serde::Serialize as _;

const RESPONSE_CODE_KEY: &str = "responseCode";
const RESPONSE_MESSAGE_KEY: &str = "responseMessage";
const SUCCESS_MESSAGE: &str = "Successful";

/// A SNAP BI response with locally contradictory states excluded.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SnapResponse<T> {
    /// Valid success code plus a typed payload.
    Success(SuccessResponse<T>),
    /// Valid non-success code plus stable classification.
    Failure(FailureResponse),
    /// Malformed upstream code retained verbatim.
    Malformed(MalformedResponse),
}

impl<T> SnapResponse<T> {
    /// Build a canonical HTTP 200 / case 00 success.
    pub fn success(payload: T, service: ServiceCode) -> Result<Self, PayloadError>
    where
        T: serde::Serialize,
    {
        Self::success_with_case(payload, service, CaseCode::ZERO)
    }

    /// Build an HTTP 200 success with an explicit case code.
    pub fn success_with_case(payload: T, service: ServiceCode, case: CaseCode) -> Result<Self, PayloadError>
    where
        T: serde::Serialize,
    {
        PayloadObject::new(&payload)?;
        Ok(Self::Success(SuccessResponse {
            code: ValidResponseCode::success(service, case),
            message: SUCCESS_MESSAGE.to_owned(),
            payload,
        }))
    }

    /// Build a classified failure.
    #[must_use]
    pub fn failure(error: Error, service: ServiceCode) -> Self {
        let class = error.class();
        let code = error.response_code(service);
        let message = error.response_message();
        #[cfg(feature = "crypto")]
        let crypto_class = error.crypto_class();

        Self::Failure(FailureResponse {
            code,
            message,
            class: Some(class),
            payload: PayloadObject::empty(),
            #[cfg(feature = "crypto")]
            crypto_class,
        })
    }

    /// HTTP status safe to apply to a framework response.
    ///
    /// Malformed wire codes always return 500.
    #[must_use]
    pub fn http_status(&self) -> http::StatusCode {
        match self {
            Self::Success(response) => response.code.http_status(),
            Self::Failure(response) => response.code.http_status(),
            Self::Malformed(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Verbatim `responseCode`.
    #[must_use]
    pub fn response_code(&self) -> &str {
        match self {
            Self::Success(response) => response.code.as_str(),
            Self::Failure(response) => response.code.as_str(),
            Self::Malformed(response) => response.code.as_str(),
        }
    }

    /// Human-readable wire message.
    #[must_use]
    pub fn response_message(&self) -> &str {
        match self {
            Self::Success(response) => &response.message,
            Self::Failure(response) => &response.message,
            Self::Malformed(response) => &response.message,
        }
    }

    /// Fully valid code, absent only for malformed input.
    #[must_use]
    pub const fn valid_response_code(&self) -> Option<&ValidResponseCode> {
        match self {
            Self::Success(response) => Some(&response.code),
            Self::Failure(response) => Some(&response.code),
            Self::Malformed(_) => None,
        }
    }

    /// Malformed raw code, when present.
    #[must_use]
    pub const fn raw_response_code(&self) -> Option<&RawResponseCode> {
        match self {
            Self::Success(_) | Self::Failure(_) => None,
            Self::Malformed(response) => Some(&response.code),
        }
    }

    /// Typed success payload.
    #[must_use]
    pub const fn payload(&self) -> Option<&T> {
        match self {
            Self::Success(response) => Some(&response.payload),
            Self::Failure(_) | Self::Malformed(_) => None,
        }
    }

    /// Stable failure class, when known.
    #[must_use]
    pub const fn error_class(&self) -> Option<ErrorClass> {
        match self {
            Self::Failure(response) => response.class,
            Self::Success(_) | Self::Malformed(_) => None,
        }
    }
}

/// Valid success state.
#[derive(Debug, Clone, PartialEq)]
pub struct SuccessResponse<T> {
    code: ValidResponseCode,
    message: String,
    payload: T,
}

impl<T> SuccessResponse<T> {
    /// Valid response code.
    #[must_use]
    pub const fn response_code(&self) -> &ValidResponseCode {
        &self.code
    }

    /// Wire response message.
    #[must_use]
    pub fn response_message(&self) -> &str {
        &self.message
    }

    /// Typed payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consume the response and return its payload.
    #[must_use]
    pub fn into_payload(self) -> T {
        self.payload
    }
}

/// Valid non-success state.
#[derive(Debug, Clone, PartialEq)]
pub struct FailureResponse {
    code: ValidResponseCode,
    message: String,
    class: Option<ErrorClass>,
    payload: PayloadObject,
    #[cfg(feature = "crypto")]
    crypto_class: Option<crate::CryptoFailureClass>,
}

impl FailureResponse {
    /// Valid response code.
    #[must_use]
    pub const fn response_code(&self) -> &ValidResponseCode {
        &self.code
    }

    /// Wire response message.
    #[must_use]
    pub fn response_message(&self) -> &str {
        &self.message
    }

    /// Stable taxonomy class, absent for unknown valid codes.
    #[must_use]
    pub const fn error_class(&self) -> Option<ErrorClass> {
        self.class
    }

    /// Unclassified extra wire fields.
    #[must_use]
    pub const fn raw_payload(&self) -> &PayloadObject {
        &self.payload
    }

    /// Crypto-specific operational class for locally converted errors.
    #[cfg(feature = "crypto")]
    #[must_use]
    pub const fn crypto_class(&self) -> Option<crate::CryptoFailureClass> {
        self.crypto_class
    }
}

/// Malformed upstream state retained for diagnostics and safe re-emission.
#[derive(Debug, Clone, PartialEq)]
pub struct MalformedResponse {
    code: RawResponseCode,
    message: String,
    payload: PayloadObject,
}

impl MalformedResponse {
    /// Verbatim malformed code.
    #[must_use]
    pub const fn response_code(&self) -> &RawResponseCode {
        &self.code
    }

    /// Wire response message.
    #[must_use]
    pub fn response_message(&self) -> &str {
        &self.message
    }

    /// Unclassified extra wire fields.
    #[must_use]
    pub const fn raw_payload(&self) -> &PayloadObject {
        &self.payload
    }
}

/// A flat payload map that cannot contain envelope keys.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PayloadObject(serde_json::Map<String, serde_json::Value>);

impl PayloadObject {
    /// Serialize and validate a typed payload.
    ///
    /// JSON objects are accepted. JSON null represents a fieldless payload.
    pub fn new<T: serde::Serialize + ?Sized>(payload: &T) -> Result<Self, PayloadError> {
        match serde_json::to_value(payload).map_err(PayloadError::Serialization)? {
            serde_json::Value::Object(map) => Self::from_map(map),
            serde_json::Value::Null => Ok(Self::empty()),
            _ => Err(PayloadError::NotObject),
        }
    }

    /// Validate an existing JSON object.
    pub fn from_map(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, PayloadError> {
        for key in [RESPONSE_CODE_KEY, RESPONSE_MESSAGE_KEY] {
            if map.contains_key(key) {
                return Err(PayloadError::ReservedKey { key });
            }
        }
        Ok(Self(map))
    }

    /// Empty field set.
    #[must_use]
    pub fn empty() -> Self {
        Self(serde_json::Map::new())
    }

    /// Read-only JSON fields.
    #[must_use]
    pub const fn as_map(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.0
    }

    /// Consume and return the JSON fields.
    #[must_use]
    pub fn into_map(self) -> serde_json::Map<String, serde_json::Value> {
        self.0
    }
}

/// Typed payload validation failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PayloadError {
    /// A top-level payload key collides with the envelope.
    #[error("payload key `{key}` is reserved by the SNAP BI envelope")]
    ReservedKey {
        /// Colliding key.
        key: &'static str,
    },
    /// A flat response cannot contain a scalar or sequence payload.
    #[error("SNAP BI payload must serialize as a JSON object or null")]
    NotObject,
    /// Typed payload serialization failed.
    #[error("SNAP BI payload serialization failed: {0}")]
    Serialization(#[source] serde_json::Error),
}

/// The wire body for an internal error, for a caller that cannot return one.
///
/// A framework's response conversion has no way to fail, so an adapter whose own serialization
/// failed needs bytes rather than a `Result`. It lives here because both adapters need the same
/// bytes, and a copy in each is a copy of this crate's own wire format.
///
/// The fallback is DERIVED from the response it is falling back for. Spelled out, it pins one
/// service code -- and this function takes one, so every caller passing another would have been
/// answered with the wrong `responseCode` on that path.
#[must_use]
pub fn internal_error_body(service: ServiceCode) -> Vec<u8> {
    let response = SnapResponse::<()>::failure(Error::InternalServerError, service);
    serde_json::to_vec(&response).unwrap_or_else(|_| {
        format!(
            r#"{{"{RESPONSE_CODE_KEY}":"{}","{RESPONSE_MESSAGE_KEY}":"{}"}}"#,
            response.response_code(),
            response.response_message()
        )
        .into_bytes()
    })
}

impl<T> serde::Serialize for SnapResponse<T>
where
    T: serde::Serialize,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Success(response) => {
                let payload = PayloadObject::new(&response.payload).map_err(serde::ser::Error::custom)?;
                serialize_wire(response.code.as_str(), &response.message, &payload, serializer)
            }
            Self::Failure(response) => {
                serialize_wire(response.code.as_str(), &response.message, &response.payload, serializer)
            }
            Self::Malformed(response) => {
                serialize_wire(response.code.as_str(), &response.message, &response.payload, serializer)
            }
        }
    }
}

impl<'de, T> serde::Deserialize<'de> for SnapResponse<T>
where
    T: serde::de::DeserializeOwned,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireResponse::deserialize(deserializer)?;
        let payload = PayloadObject::from_map(wire.payload).map_err(serde::de::Error::custom)?;

        match ResponseCode::parse(wire.response_code).into_result() {
            Ok(code) if code.http_status().is_success() => {
                let payload = deserialize_success_payload(payload).map_err(serde::de::Error::custom)?;
                Ok(Self::Success(SuccessResponse { code, message: wire.response_message, payload }))
            }
            Ok(code) => {
                let class = code.classify();
                Ok(Self::Failure(FailureResponse {
                    code,
                    message: wire.response_message,
                    class,
                    payload,
                    #[cfg(feature = "crypto")]
                    crypto_class: None,
                }))
            }
            Err(code) => {
                Ok(Self::Malformed(MalformedResponse { code, message: wire.response_message, payload }))
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse {
    response_code: String,
    response_message: String,
    #[serde(flatten)]
    payload: serde_json::Map<String, serde_json::Value>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WireResponseRef<'a> {
    response_code: &'a str,
    response_message: &'a str,
    #[serde(flatten)]
    payload: &'a serde_json::Map<String, serde_json::Value>,
}

fn serialize_wire<S: serde::Serializer>(
    code: &str,
    message: &str,
    payload: &PayloadObject,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    WireResponseRef { response_code: code, response_message: message, payload: payload.as_map() }
        .serialize(serializer)
}

fn deserialize_success_payload<T: serde::de::DeserializeOwned>(
    payload: PayloadObject,
) -> Result<T, serde_json::Error> {
    let map = payload.into_map();
    let object = serde_json::Value::Object(map.clone());
    match serde_json::from_value(object) {
        Ok(payload) => Ok(payload),
        Err(object_error) if map.is_empty() => {
            serde_json::from_value(serde_json::Value::Null).map_err(|_| object_error)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod internal_error_body_tests {
    use super::*;

    /// The body an adapter emits must carry the SERVICE CODE it was given. A spelled-out fallback
    /// pinned one, so every other service code would have been answered wrongly on that path.
    #[test]
    fn the_body_carries_the_service_code_it_was_given() {
        let mut checked = 0_usize;
        for raw in [0_u8, 17, 99] {
            let service = ServiceCode::new(raw).expect("the fixture is a service code");
            let body = internal_error_body(service);
            let parsed: serde_json::Value = serde_json::from_slice(&body).expect("the body is JSON");
            let expected = SnapResponse::<()>::failure(Error::InternalServerError, service);
            assert_eq!(
                expected.response_code(),
                parsed[RESPONSE_CODE_KEY].as_str().expect("the body names a response code"),
                "service {raw:02}"
            );
            assert_eq!(
                expected.response_message(),
                parsed[RESPONSE_MESSAGE_KEY].as_str().expect("the body names a message")
            );
            checked += 1;
        }
        assert_eq!(3, checked);
    }
}
