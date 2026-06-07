//! Wire-level `responseCode` newtype + the `ServiceCode` constrained `u8`.
//!
//! SNAP BI's `responseCode` is a 7-digit string: `HHHSSCC` where `HHH` is the
//! HTTP status (3 digits), `SS` is the service code (2 digits, 00–99), and
//! `CC` is the case code (2 digits). [`ResponseCode::parse`] is total — it
//! never errors. Malformed codes preserve `raw()` while `http()` / `service()`
//! / `case()` return `None`, keeping the `responseMessage` available for
//! operator inspection (review F-03 fix).

use crate::error::Error;

/// Wire-format `responseCode`.
///
/// Construct via [`ResponseCode::parse`] (defensive — never fails) or via
/// [`ResponseCode::from_parts`] (validated). The raw string is preserved
/// verbatim even when malformed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ResponseCode(String);

impl ResponseCode {
    /// Wrap an arbitrary string. Never fails — the value is preserved as-is.
    pub fn parse(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Construct from validated `(http, service, case)` parts. Panics if `http`
    /// is outside `100..=999`; use [`ResponseCode::parse`] when you cannot
    /// guarantee the inputs.
    pub fn from_parts(http: u16, service: ServiceCode, case: u8) -> Self {
        assert!((100..=999).contains(&http), "http status out of range");
        Self(format!("{http:03}{:02}{case:02}", service.get()))
    }

    /// Construct a `2-prefix` success code (`2HH SSCC` with `HH=00`).
    pub fn success(service: ServiceCode, sub: u8) -> Self {
        Self::from_parts(200, service, sub)
    }

    /// The verbatim wire string.
    pub fn raw(&self) -> &str {
        &self.0
    }

    /// Extract the leading HTTP status, defensively. `None` if the code is not
    /// in the canonical 7-digit form or the leading 3 digits don't form a valid
    /// HTTP status.
    pub fn http(&self) -> Option<http::StatusCode> {
        if self.0.len() != 7 {
            return None;
        }
        let n = self.0.get(..3)?.parse::<u16>().ok()?;
        http::StatusCode::from_u16(n).ok()
    }

    /// Extract the service code (positions 4..5). `None` for malformed codes.
    pub fn service(&self) -> Option<ServiceCode> {
        if self.0.len() != 7 {
            return None;
        }
        let n = self.0.get(3..5)?.parse::<u8>().ok()?;
        ServiceCode::new(n)
    }

    /// Extract the case code (positions 6..7). `None` for malformed codes.
    pub fn case(&self) -> Option<u8> {
        if self.0.len() != 7 {
            return None;
        }
        self.0.get(5..7)?.parse::<u8>().ok()
    }

    /// Inverse classifier: given a received wire code, return the matching
    /// [`Error`] variant (with empty-string payload where the variant carries
    /// one). `None` for unknown HTTP+case pairs and for malformed wire forms.
    ///
    /// Designed for client-side use — receiving a response and reasoning about
    /// it typed.
    pub fn classify(&self) -> Option<Error> {
        let http = self.http()?.as_u16();
        let case = self.case()?;
        Error::from_http_and_case(http, case)
    }
}

impl core::fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// SNAP BI service code (2-digit slot inside `responseCode`).
///
/// Construction is total via [`ServiceCode::new`]: values `>99` return `None`,
/// closing the truncation hazard of the old `service_code % 100` formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceCode(u8);

impl ServiceCode {
    /// Construct a `ServiceCode`. Returns `None` for values `>99`.
    pub const fn new(code: u8) -> Option<Self> {
        if code > 99 { None } else { Some(Self(code)) }
    }

    /// The underlying value.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl core::fmt::Display for ServiceCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02}", self.0)
    }
}

impl serde::Serialize for ServiceCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ServiceCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let n = u8::deserialize(deserializer)?;
        ServiceCode::new(n).ok_or_else(|| serde::de::Error::custom("service code must be 0..=99"))
    }
}
