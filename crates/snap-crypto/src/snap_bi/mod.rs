//! Bank Indonesia SNAP BI recipe helpers.
//!
//! Everything in this module is *spec-prescribed plumbing*: stringToSign
//! canonical formats, SHA-256 lowercase-hex body hashing, ISO-8601 Jakarta
//! timestamps, and canonical header sets. Centralising the recipe here means
//! every PT IMMER consumer can avoid reinventing the colon-separator,
//! lowercase-hex, or timezone offset rules — each of which has historically
//! produced opaque `4011100` failures in production.
//!
//! Feature-gated behind `snap-bi` (default on). Pulls `chrono` (timestamps) and
//! `http` (Method / HeaderMap types).

pub mod hash;
pub mod headers;
pub mod sign;
pub mod string_to_sign;
pub mod timestamp;

pub use hash::{sha256_lower_hex, sha512_lower_hex};
pub use headers::{OAuthHeaders, ServiceHeaders};
pub use sign::{sign_oauth, sign_service, verify_oauth, verify_service};
pub use string_to_sign::{OAuthStringToSign, ServiceStringToSign};
pub use timestamp::{Precision, format_jakarta, now_jakarta_ms, now_jakarta_seconds};
