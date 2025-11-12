#[cfg(all(feature = "systemd", feature = "wasm32"))]
compile_error!("Feature \"systemd\" can't be combined with \"wasm32\".");

#[cfg(all(feature = "with-actix-web", feature = "wasm32"))]
compile_error!("Feature \"with-actix-web\" can't be combined with \"wasm32\".");

#[cfg(not(any(feature = "systemd", feature = "wasm32")))]
compile_error!("At least feature \"systemd\" or \"wasm32\" must be enabled.");

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

#[cfg(feature = "systemd")]
fn init_systemd() -> std::result::Result<(), Error> {
    tracing_log::LogTracer::init()?;
    let filter_layer = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(TRACING_FILTER));
    let subscriber = tracing_subscriber::layer::SubscriberExt::with(
        tracing_subscriber::registry(),
        filter_layer,
    );

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
        tracing::subscriber::set_global_default(tracing_subscriber::layer::SubscriberExt::with(
            subscriber,
            journald_layer,
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
pub fn get_actix_web_logger()
-> tracing_actix_web::TracingLogger<tracing_actix_web::DefaultRootSpanBuilder> {
    tracing_actix_web::TracingLogger::default()
}
