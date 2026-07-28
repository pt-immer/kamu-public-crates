//! Pure codec and validation for the 18-byte PostgreSQL payload.

use core::fmt;

use kamu_money_core::Iso4217;
use kamu_money_core::advanced::domain;

/// Bytes in the fixed-width PostgreSQL and binary-wire representation.
pub(crate) const PAYLOAD_BYTES: usize = 18;

/// Raw storage bytes: 16 little-endian amount bytes, then a little-endian ISO code.
///
/// A `Payload` is structural, not semantic. Call [`validate_payload`] before
/// treating its bytes as money.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct Payload([u8; PAYLOAD_BYTES]);

const _: () = assert!(size_of::<Payload>() == PAYLOAD_BYTES);
const _: () = assert!(align_of::<Payload>() == 1);

impl Payload {
    pub(crate) fn from_parts(units: i128, code: u16) -> Self {
        let mut bytes = [0_u8; PAYLOAD_BYTES];
        bytes[..16].copy_from_slice(&units.to_le_bytes());
        bytes[16..].copy_from_slice(&code.to_le_bytes());
        Self(bytes)
    }

    pub(crate) const fn from_bytes(bytes: [u8; PAYLOAD_BYTES]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn to_bytes(self) -> [u8; PAYLOAD_BYTES] {
        self.0
    }

    pub(crate) fn units(self) -> i128 {
        let mut units = [0_u8; 16];
        units.copy_from_slice(&self.0[..16]);
        i128::from_le_bytes(units)
    }

    pub(crate) const fn code(self) -> u16 {
        u16::from_le_bytes([self.0[16], self.0[17]])
    }
}

impl TryFrom<&[u8]> for Payload {
    type Error = PayloadLengthError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; PAYLOAD_BYTES] =
            bytes.try_into().map_err(|_| PayloadLengthError { actual: bytes.len() })?;
        Ok(Self::from_bytes(bytes))
    }
}

/// A payload whose currency, optional expected currency, and amount domain are valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ValidatedAmount {
    payload: Payload,
    currency: Iso4217,
}

impl ValidatedAmount {
    pub(crate) const fn payload(self) -> Payload {
        self.payload
    }

    pub(crate) fn units(self) -> i128 {
        self.payload.units()
    }

    pub(crate) const fn currency(self) -> Iso4217 {
        self.currency
    }
}

/// Validate one raw payload at every mixed, wire, and datum-to-operation edge.
pub(crate) fn validate_payload(
    payload: Payload,
    expected: Option<Iso4217>,
) -> Result<ValidatedAmount, ValidationError> {
    if let Some(expected) = expected
        && payload.code() != expected.numeric()
    {
        return Err(ValidationError::UnexpectedCurrency { expected, found_code: payload.code() });
    }

    let currency = Iso4217::from_numeric(payload.code())
        .ok_or(ValidationError::UnknownCurrency { code: payload.code() })?;
    if !domain::in_domain(payload.units()) {
        return Err(ValidationError::OutOfDomain { units: payload.units(), currency });
    }

    Ok(ValidatedAmount { payload, currency })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PayloadLengthError {
    pub(crate) actual: usize,
}

impl fmt::Display for PayloadLengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "payload is {} bytes; expected {PAYLOAD_BYTES}", self.actual)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValidationError {
    UnexpectedCurrency { expected: Iso4217, found_code: u16 },
    UnknownCurrency { code: u16 },
    OutOfDomain { units: i128, currency: Iso4217 },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::UnexpectedCurrency { expected, found_code } => {
                write!(f, "expected {}, found ", expected.alpha3())?;
                if let Some(found) = Iso4217::from_numeric(found_code) {
                    f.write_str(found.alpha3())
                } else {
                    write!(f, "<unknown code {found_code}>")
                }
            }
            Self::UnknownCurrency { code } => {
                write!(f, "stored ISO 4217 numeric code {code} is not in kamu_money_core's table")
            }
            Self::OutOfDomain { units, currency } => {
                write!(
                    f,
                    "stored {} amount with {units} units is outside the domain |units| <= 10^36 - 1",
                    currency.alpha3()
                )
            }
        }
    }
}
