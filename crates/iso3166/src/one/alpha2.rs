use core::fmt;
use core::str::FromStr;

use crate::error::ParseCountryError;
use crate::one::{Alpha2, Alpha3, Numeric};

impl Alpha2 {
    /// Canonical uppercase two-letter string form (e.g. `"ID"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.as_str_generated()
    }

    /// English short name (e.g. `"Indonesia"`).
    #[must_use]
    pub const fn short_name(self) -> &'static str {
        self.short_name_generated()
    }

    /// English long / official name (e.g. `"Republic of Indonesia"`).
    #[must_use]
    pub const fn official_name(self) -> &'static str {
        self.official_name_generated()
    }

    /// Convert to the corresponding [`Alpha3`] code (infallible).
    #[must_use]
    pub const fn to_alpha3(self) -> Alpha3 {
        self.to_alpha3_generated()
    }

    /// Convert to the corresponding [`Numeric`] code (infallible).
    #[must_use]
    pub const fn to_numeric(self) -> Numeric {
        // Safety by construction: discriminant equals ISO numeric code.
        Numeric::new_unchecked(self as u16)
    }

    /// Parse a two-letter code, case-insensitively.
    ///
    /// # Errors
    /// Returns [`ParseCountryError::InvalidLength`] if the input is not
    /// exactly two bytes, [`ParseCountryError::NonAscii`] for non-ASCII input,
    /// or [`ParseCountryError::InvalidAlpha2`] for unknown codes.
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, ParseCountryError> {
        if bytes.len() != 2 {
            return Err(ParseCountryError::InvalidLength { expected: 2, got: bytes.len() });
        }
        let mut upper = [0u8; 2];
        for (i, &b) in bytes.iter().enumerate() {
            if !b.is_ascii() {
                return Err(ParseCountryError::NonAscii);
            }
            upper[i] = b.to_ascii_uppercase();
        }
        let key = core::str::from_utf8(&upper).map_err(|_| ParseCountryError::NonAscii)?;
        crate::one::ALPHA2_BY_STR.get(key).copied().ok_or(ParseCountryError::InvalidAlpha2)
    }

    /// Parse a two-letter code, case-insensitively.
    ///
    /// # Errors
    /// See [`Alpha2::try_from_bytes`].
    pub fn try_from_str(s: &str) -> Result<Self, ParseCountryError> {
        Self::try_from_bytes(s.as_bytes())
    }

    /// Subdivisions (ISO 3166-2) whose parent is this country, in deterministic order.
    ///
    /// Returns an empty slice if the country has no subdivisions in the vendored dataset.
    #[must_use]
    pub fn subdivisions(self) -> &'static [crate::two::Subdivision] {
        crate::two::subdivisions_of(self)
    }
}

impl fmt::Display for Alpha2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Alpha2 {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for Alpha2 {
    type Err = ParseCountryError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
    }
}

impl TryFrom<&str> for Alpha2 {
    type Error = ParseCountryError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from_str(s)
    }
}

impl From<Alpha2> for Alpha3 {
    fn from(a: Alpha2) -> Alpha3 {
        a.to_alpha3()
    }
}

impl From<Alpha2> for Numeric {
    fn from(a: Alpha2) -> Numeric {
        a.to_numeric()
    }
}

impl From<Alpha2> for u16 {
    fn from(a: Alpha2) -> u16 {
        a as u16
    }
}
