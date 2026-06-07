//! Wire envelope + the generic [`SnapResponse<T>`].
//!
//! The wire shape is a flat JSON object combining envelope fields
//! (`responseCode`, `responseMessage`) with the payload fields. On the Rust
//! side these are split: [`SnapEnvelope`] is the typed envelope; `payload: T`
//! is the typed payload (optional — absent on error responses).
//!
//! `SnapResponse<T>` uses a custom `Serialize` + `Deserialize` pair that:
//!
//! - Serialises envelope fields + payload fields into one flat JSON object.
//! - Deserialises envelope fields out, then attempts to deserialise the
//!   remaining map into `T`. On failure, **propagates the error** (review
//!   F-08). Silent `payload = None` is gone.

use crate::error::Error;
use crate::response_code::{ResponseCode, ServiceCode};

const RESPONSE_CODE_KEY: &str = "responseCode";
const RESPONSE_MESSAGE_KEY: &str = "responseMessage";

/// The typed envelope (`responseCode` + `responseMessage`).
///
/// Derived `Serialize` / `Deserialize` use `camelCase` for the wire keys.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapEnvelope {
    /// The 7-digit wire code.
    pub response_code: ResponseCode,
    /// Human-readable message that accompanies the code on the wire.
    pub response_message: String,
}

/// A full SNAP BI response: typed envelope + optional typed payload.
///
/// Use [`SnapResponse::ok`] for success responses and [`SnapResponse::err`]
/// for error responses. Both helpers require an explicit [`ServiceCode`];
/// there is no `0` default, no `From<Result>` impl, no silent fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapResponse<T> {
    /// Envelope fields.
    pub envelope: SnapEnvelope,
    /// Payload (`None` on error responses; `Some(_)` on success).
    pub payload: Option<T>,
}

impl<T> SnapResponse<T> {
    /// Build a success response. `sub` is the sub-code embedded in the
    /// trailing two digits (typically `0` for the canonical OK).
    pub fn ok(payload: T, service: ServiceCode, sub: u8) -> Self {
        Self {
            envelope: SnapEnvelope {
                response_code: ResponseCode::success(service, sub),
                response_message: "Successful".to_owned(),
            },
            payload: Some(payload),
        }
    }

    /// Build an error response from a typed [`Error`].
    pub fn err(error: Error, service: ServiceCode) -> Self {
        let response_message = error.to_string();
        let response_code = error.response_code(service);
        Self { envelope: SnapEnvelope { response_code, response_message }, payload: None }
    }

    /// Borrow the payload (if any).
    pub fn payload(&self) -> Option<&T> {
        self.payload.as_ref()
    }
}

impl<T> serde::Serialize for SnapResponse<T>
where
    T: serde::Serialize,
{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error as _;
        use serde::ser::SerializeMap;

        let env_value = serde_json::to_value(&self.envelope).map_err(S::Error::custom)?;
        let env_map = env_value
            .as_object()
            .ok_or_else(|| S::Error::custom("SnapEnvelope must serialize as a JSON object"))?
            .clone();

        let payload_map: serde_json::Map<String, serde_json::Value> = match &self.payload {
            Some(p) => match serde_json::to_value(p).map_err(S::Error::custom)? {
                serde_json::Value::Object(m) => m,
                // Unit-like payloads (`()`, empty tuple structs) serialise to
                // Null — accepted as a no-op so callers can use
                // `SnapResponse<()>` for endpoints with no body fields.
                serde_json::Value::Null => serde_json::Map::new(),
                _ => {
                    return Err(S::Error::custom(
                        "SnapResponse payload must serialize as a JSON object or null",
                    ));
                }
            },
            None => serde_json::Map::new(),
        };

        let mut map = serializer.serialize_map(Some(env_map.len() + payload_map.len()))?;
        for (k, v) in &env_map {
            map.serialize_entry(k, v)?;
        }
        for (k, v) in &payload_map {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de, T> serde::Deserialize<'de> for SnapResponse<T>
where
    T: serde::de::DeserializeOwned,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let mut map: serde_json::Map<String, serde_json::Value> =
            serde::Deserialize::deserialize(deserializer)?;

        let response_code_val =
            map.remove(RESPONSE_CODE_KEY).ok_or_else(|| D::Error::missing_field("responseCode"))?;
        let response_code: ResponseCode =
            serde_json::from_value(response_code_val).map_err(D::Error::custom)?;

        let response_message_val =
            map.remove(RESPONSE_MESSAGE_KEY).ok_or_else(|| D::Error::missing_field("responseMessage"))?;
        let response_message: String =
            serde_json::from_value(response_message_val).map_err(D::Error::custom)?;

        let envelope = SnapEnvelope { response_code, response_message };

        let payload = if map.is_empty() {
            None
        } else {
            let val = serde_json::Value::Object(map);
            match serde_json::from_value::<T>(val) {
                Ok(p) => Some(p),
                Err(e) => {
                    return Err(D::Error::custom(format!("payload deserialization failed: {e}")));
                }
            }
        };

        Ok(SnapResponse { envelope, payload })
    }
}
