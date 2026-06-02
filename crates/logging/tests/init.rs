//! `init()` contract. Each integration-test file is its own process, so the
//! process-global tracing subscriber and `log` bridge are exercised cleanly.

#![cfg(feature = "systemd")]
#![allow(missing_docs)]
#![forbid(unsafe_code)]

#[test]
fn init_is_idempotent_within_a_process() {
    // The first init installs the one-shot `log` -> `tracing` bridge. It may
    // still return `Err` in headless environments that lack a journald socket
    // (the bridge is installed before that point), so we don't assert success.
    let _first = kamu_logging::init();

    // A second init must always fail: `LogTracer` and the global default
    // subscriber are process-global one-shots.
    assert!(kamu_logging::init().is_err(), "init() must not succeed a second time in the same process",);
}
