//! `InitOptions::idempotent(true)` matches `init_or_skip()` semantics.

#![cfg(feature = "systemd")]

use kamu_logging::{InitOptions, init_with};

#[test]
fn idempotent_option_allows_double_init() {
    init_with(InitOptions::default().idempotent(true)).expect("first init succeeds");
    init_with(InitOptions::default().idempotent(true)).expect("second init succeeds");
}
