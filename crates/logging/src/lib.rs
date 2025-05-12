#[cfg(all(feature = "systemd", feature = "wasm32"))]
compile_error!("Feature \"systemd\" can't be combined with \"wasm32\".");

#[cfg(not(any(feature = "systemd", feature = "wasm32")))]
compile_error!("At least feature \"systemd\" or \"wasm32\" must be enabled.");

#[cfg(debug_assertions)]
#[cfg(feature = "systemd")]
const TRACING_FILTER: &str = "debug";
#[cfg(not(debug_assertions))]
#[cfg(feature = "systemd")]
const TRACING_FILTER: &str = "info";

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
    init_wasm32()?;

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
fn init_wasm32() -> std::result::Result<(), Error> {
    console_error_panic_hook::set_once();
    wasm_tracing::set_as_global_default();

    Ok(())
}

#[cfg(feature = "logging-actix-web")]
pub fn get_actix_web_logger()
-> tracing_actix_web::TracingLogger<tracing_actix_web::DefaultRootSpanBuilder> {
    tracing_actix_web::TracingLogger::default()
}
