//! `init()` must error on a second call.

#![cfg(feature = "systemd")]

use kamu_logging::{Error, init};

#[test]
fn second_init_returns_already_initialized() {
    init().expect("first init succeeds");
    let err = init().expect_err("second init must fail");
    assert!(matches!(err, Error::AlreadyInitialized), "expected AlreadyInitialized, got {err:?}",);
}
