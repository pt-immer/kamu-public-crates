//! ISO 3166-1 numeric code newtype.

use core::fmt;
use core::str::FromStr;

use crate::error::ParseCountryError;
use crate::one::{Alpha2, Alpha3};

/// ISO 3166-1 numeric country code (`0..=999`).
///
/// Stored as a `u16`. [`Display`](core::fmt::Display) renders the canonical
/// zero-padded three-digit form (e.g. `"004"` for Afghanistan).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Numeric(u16);

impl Numeric {
    /// Construct a [`Numeric`] from a validated `u16` without checking it
    /// corresponds to an assigned country code.
    ///
    /// This is `const` and intentionally skips the assigned-code lookup so
    /// that it can be called from `const fn` conversion helpers where the
    /// caller has already proven validity (e.g. converting from an
    /// [`Alpha2`] discriminant).
    #[doc(hidden)]
    #[must_use]
    pub const fn new_unchecked(value: u16) -> Self {
        Numeric(value)
    }

    /// Return the underlying `u16` value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Convert to [`Alpha2`] if this numeric code is assigned.
    #[must_use]
    pub const fn to_alpha2(self) -> Option<Alpha2> {
        crate::one::numeric_to_alpha2_generated(self.0)
    }

    /// Convert to [`Alpha3`] if this numeric code is assigned.
    #[must_use]
    pub const fn to_alpha3(self) -> Option<Alpha3> {
        crate::one::numeric_to_alpha3_generated(self.0)
    }

    /// Try to construct a [`Numeric`] from an integer, validating that it
    /// corresponds to an assigned ISO 3166-1 numeric country code.
    ///
    /// # Errors
    /// Returns [`ParseCountryError::NumericOutOfRange`] for values > 999, or
    /// [`ParseCountryError::InvalidNumeric`] for unassigned codes.
    pub const fn try_from_u16(value: u16) -> Result<Self, ParseCountryError> {
        if value > 999 {
            return Err(ParseCountryError::NumericOutOfRange);
        }
        if crate::one::numeric_to_alpha2_generated(value).is_some() {
            Ok(Numeric(value))
        } else {
            Err(ParseCountryError::InvalidNumeric)
        }
    }

    /// Parse a numeric country code from a decimal string.
    ///
    /// Accepts any number of leading zeros, e.g. `"360"`, `"0360"`, `"00360"`
    /// all yield the same value.
    ///
    /// # Errors
    /// See [`Numeric::try_from_u16`] plus [`ParseCountryError::NotAnInteger`]
    /// and [`ParseCountryError::NonAscii`].
    pub fn try_from_str(s: &str) -> Result<Self, ParseCountryError> {
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Err(ParseCountryError::NotAnInteger);
        }
        if !bytes.iter().all(u8::is_ascii_digit) {
            return Err(if bytes.iter().all(u8::is_ascii) {
                ParseCountryError::NotAnInteger
            } else {
                ParseCountryError::NonAscii
            });
        }
        // Strip leading zeros but keep at least one digit.
        let first_nonzero = bytes.iter().position(|&b| b != b'0').unwrap_or(bytes.len() - 1);
        let trimmed = &bytes[first_nonzero..];
        if trimmed.len() > 3 {
            return Err(ParseCountryError::NumericOutOfRange);
        }
        let mut value: u16 = 0;
        for &b in trimmed {
            value = value * 10 + u16::from(b - b'0');
        }
        Self::try_from_u16(value)
    }
}

impl fmt::Display for Numeric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03}", self.0)
    }
}

impl TryFrom<u16> for Numeric {
    type Error = ParseCountryError;
    fn try_from(v: u16) -> Result<Self, Self::Error> {
        Self::try_from_u16(v)
    }
}

impl FromStr for Numeric {
    type Err = ParseCountryError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
    }
}

impl TryFrom<&str> for Numeric {
    type Error = ParseCountryError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from_str(s)
    }
}

impl From<Numeric> for u16 {
    fn from(n: Numeric) -> u16 {
        n.0
    }
}

impl TryFrom<Numeric> for Alpha2 {
    type Error = ParseCountryError;
    fn try_from(n: Numeric) -> Result<Self, Self::Error> {
        n.to_alpha2().ok_or(ParseCountryError::InvalidNumeric)
    }
}

impl TryFrom<Numeric> for Alpha3 {
    type Error = ParseCountryError;
    fn try_from(n: Numeric) -> Result<Self, Self::Error> {
        n.to_alpha3().ok_or(ParseCountryError::InvalidNumeric)
    }
}
