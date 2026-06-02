#[cfg(all(feature = "systemd", feature = "wasm32"))]
compile_error!("Feature \"systemd\" can't be combined with \"wasm32\".");

#[cfg(all(feature = "with-actix-web", feature = "wasm32"))]
compile_error!("Feature \"with-actix-web\" can't be combined with \"wasm32\".");

#[cfg(not(any(feature = "systemd", feature = "wasm32")))]
compile_error!("At least feature \"systemd\" or \"wasm32\" must be enabled.");

/// basic re-exports
pub use tracing::{debug, error, info, trace, warn};

#[cfg(all(debug_assertions, feature = "systemd"))]
const TRACING_FILTER: &str = "debug";
#[cfg(all(not(debug_assertions), feature = "systemd"))]
const TRACING_FILTER: &str = "info";

#[cfg(feature = "wasm32")]
static WASM32_LOG_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(feature = "wasm32")]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    IO(#[from] std::io::Error),
    #[error("{0}")]
    TracingGlobal(#[from] tracing::subscriber::SetGlobalDefaultError),
}

#[cfg(feature = "systemd")]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    IO(#[from] std::io::Error),
    #[error("{0}")]
    TracingGlobal(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("{0}")]
    TracingLog(#[from] tracing_log::log::SetLoggerError),
}

pub fn init() -> std::result::Result<(), Error> {
    #[cfg(feature = "systemd")]
    init_systemd()?;
    #[cfg(feature = "wasm32")]
    init_wasm32();

    tracing::info!("Logging initialized");

    Ok(())
}

/// The default tracing directive when `RUST_LOG` is unset: `debug` in debug
/// builds, `info` in release builds.
#[cfg(feature = "systemd")]
fn default_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::new(TRACING_FILTER)
}

/// Resolve the env filter from `RUST_LOG`, falling back to [`default_filter`].
#[cfg(feature = "systemd")]
fn resolve_env_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter())
}

#[cfg(feature = "systemd")]
fn init_systemd() -> std::result::Result<(), Error> {
    tracing_log::LogTracer::init()?;
    let filter_layer = resolve_env_filter();
    let subscriber =
        tracing_subscriber::layer::SubscriberExt::with(tracing_subscriber::registry(), filter_layer);

    if console::Term::stdout().is_term() {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_ansi(true)
            .with_line_number(true)
            .with_thread_ids(true);
        tracing::subscriber::set_global_default(tracing_subscriber::layer::SubscriberExt::with(
            subscriber, fmt_layer,
        ))?;
    } else {
        let journald_layer = tracing_journald::layer()?;
        let fmt_layer = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(std::io::stderr);
        let subscriber_with_journald =
            tracing_subscriber::layer::SubscriberExt::with(subscriber, journald_layer);
        tracing::subscriber::set_global_default(tracing_subscriber::layer::SubscriberExt::with(
            subscriber_with_journald,
            fmt_layer,
        ))?;
    }

    Ok(())
}

#[cfg(feature = "wasm32")]
fn init_wasm32() {
    let _ = WASM32_LOG_INIT.get_or_init(|| {
        console_error_panic_hook::set_once();
        let _ = wasm_tracing::try_set_as_global_default();
    });
}

#[cfg(feature = "with-actix-web")]
pub fn get_actix_web_logger() -> tracing_actix_web::TracingLogger<tracing_actix_web::DefaultRootSpanBuilder> {
    tracing_actix_web::TracingLogger::default()
}

#[cfg(all(test, feature = "systemd"))]
mod tests {
    use super::*;

    #[test]
    fn tracing_filter_matches_build_profile() {
        #[cfg(debug_assertions)]
        assert_eq!(TRACING_FILTER, "debug");
        #[cfg(not(debug_assertions))]
        assert_eq!(TRACING_FILTER, "info");
    }

    #[test]
    fn default_filter_encodes_the_constant() {
        let shown = default_filter().to_string();
        assert!(
            shown.to_ascii_lowercase().contains(TRACING_FILTER),
            "default filter {shown:?} should encode {TRACING_FILTER:?}",
        );
    }

    #[test]
    fn resolve_env_filter_is_usable_regardless_of_environment() {
        // Whether or not RUST_LOG is set in the test environment, this must
        // yield a usable, non-empty filter.
        assert!(!resolve_env_filter().to_string().is_empty());
    }

    #[test]
    fn global_default_error_is_wrapped_and_displayed() {
        use tracing::subscriber::set_global_default;
        // Only this test installs the process-global subscriber, so its first
        // call is the process's first and always succeeds; the second fails.
        set_global_default(tracing_subscriber::registry()).expect("first set_global_default");
        let raw = set_global_default(tracing_subscriber::registry())
            .expect_err("a second set_global_default must fail");
        let err = Error::from(raw);
        assert!(matches!(err, Error::TracingGlobal(_)));
        assert!(!err.to_string().is_empty());
        assert!(format!("{err:?}").contains("TracingGlobal"));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn log_tracer_error_is_wrapped_and_displayed() {
        // The `log` global logger is a separate one-shot; only this test installs it.
        tracing_log::LogTracer::init().expect("first LogTracer::init");
        let raw = tracing_log::LogTracer::init().expect_err("a second LogTracer::init must fail");
        let err = Error::from(raw);
        assert!(matches!(err, Error::TracingLog(_)));
        assert!(!err.to_string().is_empty());
        assert!(format!("{err:?}").contains("TracingLog"));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[cfg(feature = "with-actix-web")]
    #[test]
    fn actix_web_logger_is_constructible() {
        let _logger = get_actix_web_logger();
    }
}
