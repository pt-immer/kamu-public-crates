//! `Error` ergonomics: `Display`, `Debug`, and `std::error::Error::source`.

#![cfg(feature = "systemd")]
#![allow(missing_docs)]
#![forbid(unsafe_code)]

use std::error::Error as _;

use kamu_logging::Error;

#[test]
fn io_variant_displays_and_exposes_source() {
    let err = Error::from(std::io::Error::other("disk on fire"));
    assert!(err.to_string().contains("disk on fire"), "Display should forward the inner message");
    assert!(format!("{err:?}").contains("IO"), "Debug should name the variant");
    assert!(err.source().is_some(), "the `#[from]` source should be exposed via Error::source");
}
