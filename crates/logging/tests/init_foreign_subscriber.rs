//! Foreign subscriber ownership is never mistaken for idempotent success.

#![cfg(feature = "systemd")]

use kamu_logging::{Error, InitOptions, Sink, init_with};

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
fn idempotence_rejects_a_foreign_global_subscriber_without_claiming_log() {
    tracing::subscriber::set_global_default(tracing_subscriber::registry())
        .expect("test owns fresh subscriber slot");

    let options = InitOptions::default().with_sink(Sink::Stderr).idempotent(true);
    #[cfg(feature = "with-otlp")]
    let options = options.with_otlp(kamu_logging::otlp::OtlpConfig::new("http://127.0.0.1:9"));

    let error = init_with(options).expect_err("foreign subscriber must not be skipped");
    assert!(matches!(error, Error::ForeignGlobalSubscriber));

    #[cfg(feature = "with-otlp")]
    {
        kamu_logging::flush_otlp().expect("uncommitted provider must not be stored");
        kamu_logging::shutdown_otlp().expect("uncommitted provider must not be stored");
    }

    tracing_log::log::set_logger(&TEST_LOGGER).expect("failed subscriber commit must not install LogTracer");
}
