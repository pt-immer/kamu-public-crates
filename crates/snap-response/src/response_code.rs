//! SNAP BI `responseCode` parsing and construction.

use crate::ErrorClass;

const WIRE_LEN: usize = 7;

/// A syntactically valid `HHHSSCC` response code.
///
/// All accessors read values captured by one full parse. A code is never
/// partially valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidResponseCode {
    raw: String,
    http: http::StatusCode,
    service: ServiceCode,
    case: CaseCode,
}

impl ValidResponseCode {
    /// Parse all seven ASCII digits and validate the HTTP status.
    pub fn parse(raw: impl Into<String>) -> Result<Self, ResponseCodeError> {
        let raw = raw.into();
        let bytes = raw.as_bytes();

        if bytes.len() != WIRE_LEN {
            return Err(ResponseCodeError::Length { actual: bytes.len() });
        }
        if let Some(index) = bytes.iter().position(|byte| !byte.is_ascii_digit()) {
            return Err(ResponseCodeError::NonDigit { index });
        }

        let http_number = digits(bytes[0], bytes[1], bytes[2]);
        let http = http::StatusCode::from_u16(http_number)
            .map_err(|_| ResponseCodeError::HttpStatus { value: http_number })?;
        let service = ServiceCode((bytes[3] - b'0') * 10 + bytes[4] - b'0');
        let case = CaseCode((bytes[5] - b'0') * 10 + bytes[6] - b'0');

        Ok(Self { raw, http, service, case })
    }

    /// Build a code from validated components.
    #[must_use]
    pub fn from_parts(http: http::StatusCode, service: ServiceCode, case: CaseCode) -> Self {
        Self { raw: format!("{}{}{case}", http.as_u16(), service), http, service, case }
    }

    /// Validate numeric service and case components, then build a code.
    pub fn try_from_parts(http: http::StatusCode, service: u8, case: u8) -> Result<Self, CodeOutOfRange> {
        Ok(Self::from_parts(http, ServiceCode::try_from(service)?, CaseCode::try_from(case)?))
    }

    /// Build an HTTP 200 success code.
    #[must_use]
    pub fn success(service: ServiceCode, case: CaseCode) -> Self {
        Self::from_parts(http::StatusCode::OK, service, case)
    }

    /// Canonical seven-digit wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// HTTP status captured by the full parse.
    #[must_use]
    pub const fn http_status(&self) -> http::StatusCode {
        self.http
    }

    /// Two-digit service component.
    #[must_use]
    pub const fn service_code(&self) -> ServiceCode {
        self.service
    }

    /// Two-digit case component.
    #[must_use]
    pub const fn case_code(&self) -> CaseCode {
        self.case
    }

    /// Classify a known SNAP BI error code.
    #[must_use]
    pub fn classify(&self) -> Option<ErrorClass> {
        ErrorClass::from_http_and_case(self.http, self.case)
    }
}

impl TryFrom<&str> for ValidResponseCode {
    type Error = ResponseCodeError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl TryFrom<String> for ValidResponseCode {
    type Error = ResponseCodeError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl core::fmt::Display for ValidResponseCode {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.raw)
    }
}

impl serde::Serialize for ValidResponseCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
}

impl<'de> serde::Deserialize<'de> for ValidResponseCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(raw).map_err(serde::de::Error::custom)
    }
}

/// An unparsed wire value retained for diagnostics and round trips.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawResponseCode(String);

impl RawResponseCode {
    fn new(raw: String) -> Self {
        Self(raw)
    }

    /// Verbatim wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for RawResponseCode {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl serde::Serialize for RawResponseCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ParsedResponseCode {
    Valid(ValidResponseCode),
    Raw(RawResponseCode),
}

/// A total wire parser containing either a fully valid code or the raw value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResponseCode(ParsedResponseCode);

impl ResponseCode {
    /// Parse a wire value without discarding malformed input.
    #[must_use]
    pub fn parse(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        match ValidResponseCode::parse(raw.clone()) {
            Ok(code) => Self(ParsedResponseCode::Valid(code)),
            Err(_) => Self(ParsedResponseCode::Raw(RawResponseCode::new(raw))),
        }
    }

    /// Verbatim wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            ParsedResponseCode::Valid(code) => code.as_str(),
            ParsedResponseCode::Raw(code) => code.as_str(),
        }
    }

    /// Fully validated representation, when present.
    #[must_use]
    pub const fn valid(&self) -> Option<&ValidResponseCode> {
        match &self.0 {
            ParsedResponseCode::Valid(code) => Some(code),
            ParsedResponseCode::Raw(_) => None,
        }
    }

    /// Raw representation when the full parse failed.
    #[must_use]
    pub const fn raw(&self) -> Option<&RawResponseCode> {
        match &self.0 {
            ParsedResponseCode::Valid(_) => None,
            ParsedResponseCode::Raw(code) => Some(code),
        }
    }

    /// HTTP status from the fully validated representation.
    #[must_use]
    pub fn http_status(&self) -> Option<http::StatusCode> {
        self.valid().map(ValidResponseCode::http_status)
    }

    /// Service component from the fully validated representation.
    #[must_use]
    pub fn service_code(&self) -> Option<ServiceCode> {
        self.valid().map(ValidResponseCode::service_code)
    }

    /// Case component from the fully validated representation.
    #[must_use]
    pub fn case_code(&self) -> Option<CaseCode> {
        self.valid().map(ValidResponseCode::case_code)
    }

    /// Classify a known SNAP BI error code.
    #[must_use]
    pub fn classify(&self) -> Option<ErrorClass> {
        self.valid().and_then(ValidResponseCode::classify)
    }

    pub(crate) fn into_result(self) -> Result<ValidResponseCode, RawResponseCode> {
        match self.0 {
            ParsedResponseCode::Valid(code) => Ok(code),
            ParsedResponseCode::Raw(code) => Err(code),
        }
    }
}

impl From<ValidResponseCode> for ResponseCode {
    fn from(code: ValidResponseCode) -> Self {
        Self(ParsedResponseCode::Valid(code))
    }
}

impl core::fmt::Display for ResponseCode {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl serde::Serialize for ResponseCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ResponseCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(String::deserialize(deserializer)?))
    }
}

/// Why a seven-character response code is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResponseCodeError {
    /// Wrong UTF-8 byte length.
    #[error("response code must contain exactly 7 ASCII digits, got {actual} bytes")]
    Length {
        /// Actual UTF-8 byte length.
        actual: usize,
    },
    /// A byte is not an ASCII digit.
    #[error("response code byte {index} is not an ASCII digit")]
    NonDigit {
        /// Zero-based byte position.
        index: usize,
    },
    /// The first three digits are outside the `http` crate's accepted range.
    #[error("response code contains invalid HTTP status {value:03}")]
    HttpStatus {
        /// Parsed numeric status.
        value: u16,
    },
}

/// A numeric component did not fit its two-digit wire slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CodeOutOfRange {
    /// Invalid service code.
    #[error("service code must be 0..=99, got {0}")]
    Service(u8),
    /// Invalid case code.
    #[error("case code must be 0..=99, got {0}")]
    Case(u8),
}

/// Two-digit SNAP BI service code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceCode(u8);

impl ServiceCode {
    /// Smallest service code.
    pub const ZERO: Self = Self(0);

    /// Construct a service code.
    #[must_use]
    pub const fn new(code: u8) -> Option<Self> {
        if code <= 99 { Some(Self(code)) } else { None }
    }

    /// Numeric value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for ServiceCode {
    type Error = CodeOutOfRange;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        Self::new(code).ok_or(CodeOutOfRange::Service(code))
    }
}

impl core::fmt::Display for ServiceCode {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{:02}", self.0)
    }
}

impl serde::Serialize for ServiceCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for ServiceCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(u8::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Two-digit SNAP BI case code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaseCode(u8);

impl CaseCode {
    /// Canonical success case.
    pub const ZERO: Self = Self(0);

    /// Construct a case code.
    #[must_use]
    pub const fn new(code: u8) -> Option<Self> {
        if code <= 99 { Some(Self(code)) } else { None }
    }

    /// Numeric value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    pub(crate) const fn from_valid(code: u8) -> Self {
        Self(code)
    }
}

impl TryFrom<u8> for CaseCode {
    type Error = CodeOutOfRange;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        Self::new(code).ok_or(CodeOutOfRange::Case(code))
    }
}

impl core::fmt::Display for CaseCode {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{:02}", self.0)
    }
}

impl serde::Serialize for CaseCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for CaseCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(u8::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

fn digits(hundreds: u8, tens: u8, ones: u8) -> u16 {
    u16::from(hundreds - b'0') * 100 + u16::from(tens - b'0') * 10 + u16::from(ones - b'0')
}
