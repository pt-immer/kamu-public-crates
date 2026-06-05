//! `init_or_skip()` must succeed on a second call.

#![cfg(feature = "systemd")]

use kamu_logging::init_or_skip;

#[test]
fn second_init_or_skip_returns_ok() {
    init_or_skip().expect("first init_or_skip succeeds");
    init_or_skip().expect("second init_or_skip must also succeed");
}
