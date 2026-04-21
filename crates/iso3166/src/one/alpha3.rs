use core::fmt;
use core::str::FromStr;

use crate::error::ParseCountryError;
use crate::one::{Alpha2, Alpha3, Numeric};

impl Alpha3 {
    /// Canonical uppercase three-letter string form (e.g. `"IDN"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.as_str_generated()
    }

    /// English short name, forwarded from the corresponding [`Alpha2`].
    #[must_use]
    pub const fn short_name(self) -> &'static str {
        self.to_alpha2().short_name()
    }

    /// English long / official name, forwarded from the corresponding [`Alpha2`].
    #[must_use]
    pub const fn official_name(self) -> &'static str {
        self.to_alpha2().official_name()
    }

    /// Convert to the corresponding [`Alpha2`] code (infallible).
    #[must_use]
    pub const fn to_alpha2(self) -> Alpha2 {
        self.to_alpha2_generated()
    }

    /// Convert to the corresponding [`Numeric`] code (infallible).
    #[must_use]
    pub const fn to_numeric(self) -> Numeric {
        Numeric::new_unchecked(self as u16)
    }

    /// Parse a three-letter code, case-insensitively.
    ///
    /// # Errors
    /// Returns [`ParseCountryError::InvalidLength`] if the input is not
    /// exactly three bytes, [`ParseCountryError::NonAscii`] for non-ASCII
    /// input, or [`ParseCountryError::InvalidAlpha3`] for unknown codes.
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, ParseCountryError> {
        if bytes.len() != 3 {
            return Err(ParseCountryError::InvalidLength { expected: 3, got: bytes.len() });
        }
        let mut upper = [0u8; 3];
        for (i, &b) in bytes.iter().enumerate() {
            if !b.is_ascii() {
                return Err(ParseCountryError::NonAscii);
            }
            upper[i] = b.to_ascii_uppercase();
        }
        let key = core::str::from_utf8(&upper).map_err(|_| ParseCountryError::NonAscii)?;
        crate::one::ALPHA3_BY_STR.get(key).copied().ok_or(ParseCountryError::InvalidAlpha3)
    }

    /// Parse a three-letter code, case-insensitively.
    ///
    /// # Errors
    /// See [`ParseCountryError`].
    pub fn try_from_str(s: &str) -> Result<Self, ParseCountryError> {
        Self::try_from_bytes(s.as_bytes())
    }
}

impl fmt::Display for Alpha3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Alpha3 {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for Alpha3 {
    type Err = ParseCountryError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
    }
}

impl TryFrom<&str> for Alpha3 {
    type Error = ParseCountryError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from_str(s)
    }
}

impl From<Alpha3> for Alpha2 {
    fn from(a: Alpha3) -> Alpha2 {
        a.to_alpha2()
    }
}

impl From<Alpha3> for Numeric {
    fn from(a: Alpha3) -> Numeric {
        a.to_numeric()
    }
}

impl From<Alpha3> for u16 {
    fn from(a: Alpha3) -> u16 {
        a as u16
    }
}
