#[cfg(debug_assertions)]
const TRACING_FILTER: &str = "debug";
#[cfg(not(debug_assertions))]
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

    tracing::info!("Logging initialized");

    Ok(())
}

#[cfg(feature = "logging-actix-web")]
pub fn get_actix_web_logger()
-> tracing_actix_web::TracingLogger<tracing_actix_web::DefaultRootSpanBuilder> {
    tracing_actix_web::TracingLogger::default()
}
