//! Foreign `log` facade ownership remains visible after subscriber commit.

#![cfg(feature = "systemd")]

use kamu_logging::{Error, InitOptions, Sink, init_or_skip, init_with};

struct TestLogger;

impl tracing_log::log::Log for TestLogger {
    fn enabled(&self, _: &tracing_log::log::Metadata<'_>) -> bool {
        false
    }

    fn log(&self, _: &tracing_log::log::Record<'_>) {}

    fn flush(&self) {}
}

static TEST_LOGGER: TestLogger = TestLogger;

#[test]
fn foreign_logger_is_distinct_and_never_becomes_idempotent_success() {
    tracing_log::log::set_logger(&TEST_LOGGER).expect("test owns fresh logger slot");

    let error = init_with(InitOptions::default().with_sink(Sink::Stderr))
        .expect_err("foreign logger must be reported");
    assert!(matches!(error, Error::ForeignGlobalLogger));

    let repeated = init_or_skip().expect_err("incomplete owned state must remain visible");
    assert!(matches!(repeated, Error::ForeignGlobalLogger));
}
