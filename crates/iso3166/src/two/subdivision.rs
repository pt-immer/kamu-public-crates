use core::fmt;
use core::str::FromStr;

use crate::error::ParseSubdivisionError;
use crate::one::Alpha2;

use super::generated::Category;

/// An ISO 3166-2 subdivision entry.
///
/// Fields are public (all `Copy`) for zero-overhead access, and stable under
/// the same vendored commit. Upstream schema changes may add fields; the
/// struct is `#[non_exhaustive]` so consumers must construct instances via
/// the generated constants rather than field literals.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subdivision {
    /// Parent country.
    pub parent: Alpha2,
    /// Canonical ISO 3166-2 code (e.g. `"ID-JK"`).
    pub code: &'static str,
    /// Subdivision name in the upstream-provided language (see [`Self::language`]).
    pub name: &'static str,
    /// ISO 639-1 language tag of the `name` field.
    pub language: &'static str,
    /// Parent subdivision ISO 3166-2 code, if any (e.g. regions that contain this province).
    pub parent_subdivision: Option<&'static str>,
    /// Category (province, state, region, …). See [`Category`].
    pub category: Category,
    /// Optional local variant / alternative spelling.
    pub local_variant: Option<&'static str>,
}

impl Subdivision {
    /// Look up a subdivision by its canonical ISO 3166-2 code, case-insensitively.
    ///
    /// # Errors
    /// See the variants of [`ParseSubdivisionError`].
    pub fn try_from_str(s: &str) -> Result<&'static Self, ParseSubdivisionError> {
        let bytes = s.as_bytes();
        if bytes.len() < 4 || bytes.len() > 6 {
            return Err(ParseSubdivisionError::InvalidLength { got: bytes.len() });
        }
        if !bytes.iter().all(u8::is_ascii) {
            return Err(ParseSubdivisionError::NonAscii);
        }
        if bytes[2] != b'-' {
            return Err(ParseSubdivisionError::MissingSeparator);
        }
        // Uppercase onto a stack buffer. Max length is 6.
        let mut upper = [0u8; 6];
        for (i, &b) in bytes.iter().enumerate() {
            upper[i] = b.to_ascii_uppercase();
        }
        let key = core::str::from_utf8(&upper[..bytes.len()])
            .map_err(|_| ParseSubdivisionError::NonAscii)?;

        // Validate parent part first so callers get a precise error.
        Alpha2::try_from_bytes(&upper[..2]).map_err(|_| ParseSubdivisionError::InvalidParent)?;

        let idx = super::SUBDIVISION_BY_CODE
            .get(key)
            .copied()
            .ok_or(ParseSubdivisionError::UnknownSubdivision)?;
        Ok(&super::ALL_SUBDIVISIONS[idx])
    }
}

impl fmt::Display for Subdivision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl FromStr for Subdivision {
    type Err = ParseSubdivisionError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s).copied()
    }
}

impl TryFrom<&str> for Subdivision {
    type Error = ParseSubdivisionError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from_str(s).copied()
    }
}

impl AsRef<str> for Subdivision {
    fn as_ref(&self) -> &str {
        self.code
    }
}
