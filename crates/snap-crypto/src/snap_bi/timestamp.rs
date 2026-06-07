//! ISO-8601 timestamp formatters for the Jakarta timezone (`+07:00`).
//!
//! Two precisions are used across the SNAP BI surface:
//!
//! - **Millisecond**: `yyyy-MM-dd'T'HH:mm:ss.SSS+07:00` — required by the OAuth
//!   `/access-token/b2b` endpoint's `X-TIMESTAMP` header.
//! - **Second**: `yyyy-MM-dd'T'HH:mm:ss+07:00` — accepted by SNAP BI service
//!   endpoints and used widely in BRI's documentation examples.
//!
//! The header and the embedded timestamp in the stringToSign must match byte
//! exactly; constructing both from the same helper avoids the most common
//! source of mismatched-signature errors.

use chrono::{DateTime, FixedOffset, Utc};

/// Timestamp precision selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// `yyyy-MM-dd'T'HH:mm:ss+07:00`.
    Seconds,
    /// `yyyy-MM-dd'T'HH:mm:ss.SSS+07:00`.
    Millis,
}

const JAKARTA_OFFSET_SECONDS: i32 = 7 * 3600;

fn jakarta() -> FixedOffset {
    FixedOffset::east_opt(JAKARTA_OFFSET_SECONDS).expect("WIB offset is well-known constant")
}

/// Current Jakarta time, millisecond precision.
pub fn now_jakarta_ms() -> String {
    format_jakarta(Utc::now().with_timezone(&jakarta()), Precision::Millis)
}

/// Current Jakarta time, second precision.
pub fn now_jakarta_seconds() -> String {
    format_jakarta(Utc::now().with_timezone(&jakarta()), Precision::Seconds)
}

/// Format an arbitrary fixed-offset `DateTime` using the selected precision.
///
/// `dt` is rendered in its own offset; pass a Jakarta-localised value if the
/// SNAP BI partner expects `+07:00`.
pub fn format_jakarta(dt: DateTime<FixedOffset>, precision: Precision) -> String {
    match precision {
        Precision::Seconds => dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        Precision::Millis => dt.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
    }
}
