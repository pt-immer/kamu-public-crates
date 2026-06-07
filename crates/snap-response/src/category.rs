//! Coarse error category — used by retry policy / logging / dashboards.

/// Bank Indonesia SNAP BI groups response codes into four broad categories.
///
/// Marked `#[non_exhaustive]` so future SNAP BI revisions can add a category
/// (e.g. `Compliance`, `LimitBreach`) without breaking consumer match arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Category {
    /// Successful outcome (`responseCode` starting with `200`).
    Success,
    /// Client message error — usually 400-class.
    Message,
    /// Business-rule violation — usually 403/404-class.
    Business,
    /// System / infrastructure error — usually 500/504-class.
    System,
}

impl Category {
    /// Stable string label for logs and dashboards.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::Message => "Message",
            Self::Business => "Business",
            Self::System => "System",
        }
    }
}

impl core::fmt::Display for Category {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
