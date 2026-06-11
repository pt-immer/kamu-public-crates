//! Subscriber construction.

use crate::{Error, InitOptions};

#[cfg(feature = "systemd")]
use std::sync::OnceLock;

#[cfg(feature = "systemd")]
static SUBSCRIBER_SET: OnceLock<()> = OnceLock::new();

#[cfg(feature = "wasm32")]
static WASM32_LOG_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Initialize the global tracing subscriber with default options.
///
/// Equivalent to `init_with(InitOptions::default())`.
///
/// # Errors
///
/// Returns [`Error::AlreadyInitialized`] if a subscriber is already set
/// (use [`init_or_skip`] or [`InitOptions::idempotent`] for test harnesses).
pub fn init() -> Result<(), Error> {
    init_with(InitOptions::default())
}

/// Initialize the global tracing subscriber, returning `Ok(())` if a
/// subscriber is already set.
///
/// Equivalent to `init_with(InitOptions::default().idempotent(true))`.
///
/// # Errors
///
/// Returns an error only on first-call failures unrelated to duplicate init
/// (e.g. systemd `LogTracer` setup failure).
pub fn init_or_skip() -> Result<(), Error> {
    init_with(InitOptions::default().idempotent(true))
}

/// Initialize the global tracing subscriber from explicit options.
///
/// Env-var overrides applied when fields are at their default `Auto` value:
///
/// - `KAMU_LOG_FORMAT` — `auto`, `compact`, `pretty`, `json`
/// - `KAMU_LOG_SINK` — `auto`, `stdout`, `stderr`, `journald`
///
/// # Errors
///
/// - [`Error::AlreadyInitialized`] if `idempotent` is `false` and a
///   subscriber is already set.
/// - [`Error::InvalidConfiguration`] if the selected target cannot honor an
///   option (for example `Sink::Journald` on wasm32).
/// - [`Error::TracingGlobal`] / `Error::TracingLog` on subscriber setup
///   failure.
/// - [`Error::IO`] if the journald socket is unavailable.
/// - `Error::OtlpInit` (with-otlp feature) if the exporter cannot be built.
pub fn init_with(options: InitOptions) -> Result<(), Error> {
    #[cfg(feature = "systemd")]
    {
        init_systemd(options)?;
    }
    #[cfg(feature = "wasm32")]
    {
        init_wasm32(options)?;
    }

    Ok(())
}

#[cfg(feature = "systemd")]
fn init_systemd(options: InitOptions) -> Result<(), Error> {
    use tracing_subscriber::layer::SubscriberExt;

    if SUBSCRIBER_SET.get().is_some() {
        return if options.idempotent { Ok(()) } else { Err(Error::AlreadyInitialized) };
    }

    if let Err(err) = tracing_log::LogTracer::init()
        && !options.idempotent
    {
        return Err(Error::from(err));
    }

    let env_var = options.resolved_env_var().to_owned();
    let default_filter = options.resolved_default_filter().to_owned();
    let filter_layer = tracing_subscriber::EnvFilter::try_from_env(&env_var)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&default_filter));

    let is_tty = console::Term::stdout().is_term();
    let effective_sink = resolve_sink(options.sink, is_tty);
    let effective_format = resolve_format(options.format, effective_sink, is_tty);

    let output_layer = build_output_layer(effective_sink, effective_format, is_tty)?;

    #[cfg(feature = "with-otlp")]
    let otlp_layer = match options.otlp.as_ref() {
        Some(cfg) => Some(crate::otlp::build_layer(cfg)?),
        None => None,
    };

    let subscriber = tracing_subscriber::registry().with(filter_layer).with(output_layer);

    #[cfg(feature = "with-otlp")]
    let subscriber = subscriber.with(otlp_layer);

    tracing::subscriber::set_global_default(subscriber)?;
    let _ = SUBSCRIBER_SET.set(());

    if let Some(name) = options.service_name.as_deref() {
        tracing::info!(service.name = %name, "Logging initialized");
    } else {
        tracing::info!("Logging initialized");
    }
    Ok(())
}

#[cfg(feature = "systemd")]
fn resolve_format(format: crate::Format, sink: crate::Sink, is_tty: bool) -> crate::Format {
    use crate::{Format, Sink};

    let mut effective = format;
    if effective == Format::Auto
        && let Ok(value) = std::env::var("KAMU_LOG_FORMAT")
    {
        effective = Format::from_env_value(&value);
    }
    if effective != Format::Auto {
        return effective;
    }
    match sink {
        Sink::Journald => Format::Compact,
        Sink::Stdout | Sink::Stderr if is_tty => Format::Pretty,
        Sink::Stdout | Sink::Stderr => Format::Compact,
        Sink::Auto => Format::Compact,
    }
}

#[cfg(feature = "systemd")]
fn resolve_sink(sink: crate::Sink, is_tty: bool) -> crate::Sink {
    use crate::Sink;

    let mut effective = sink;
    if effective == Sink::Auto
        && let Ok(value) = std::env::var("KAMU_LOG_SINK")
    {
        effective = Sink::from_env_value(&value);
    }
    if effective != Sink::Auto {
        return effective;
    }
    if is_tty { Sink::Stdout } else { Sink::Journald }
}

#[cfg(feature = "systemd")]
type DynLayer = Box<
    dyn tracing_subscriber::Layer<
            tracing_subscriber::layer::Layered<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>,
        > + Send
        + Sync,
>;

#[cfg(feature = "systemd")]
fn build_output_layer(sink: crate::Sink, format: crate::Format, is_tty: bool) -> Result<DynLayer, Error> {
    use crate::{Format, Sink};
    use tracing_subscriber::Layer;
    use tracing_subscriber::fmt;

    if sink == Sink::Journald {
        let layer = tracing_journald::layer()?;
        return Ok(Box::new(layer));
    }

    let writer: fmt::writer::BoxMakeWriter = match sink {
        Sink::Stderr => fmt::writer::BoxMakeWriter::new(std::io::stderr),
        _ => fmt::writer::BoxMakeWriter::new(std::io::stdout),
    };

    let span_events = fmt::format::FmtSpan::CLOSE;
    let with_ansi = is_tty && format != Format::Json;

    let layer: DynLayer = match format {
        Format::Json => Box::new(
            fmt::layer()
                .with_writer(writer)
                .with_span_events(span_events)
                .with_line_number(true)
                .with_thread_ids(true)
                .json()
                .with_current_span(true)
                .with_span_list(false)
                .boxed(),
        ),
        Format::Pretty => Box::new(
            fmt::layer()
                .with_writer(writer)
                .with_span_events(span_events)
                .with_ansi(with_ansi)
                .with_line_number(true)
                .with_thread_ids(true)
                .pretty()
                .boxed(),
        ),
        Format::Compact | Format::Auto => Box::new(
            fmt::layer()
                .with_writer(writer)
                .with_span_events(span_events)
                .with_ansi(with_ansi)
                .with_line_number(true)
                .with_thread_ids(true)
                .compact()
                .boxed(),
        ),
    };

    Ok(layer)
}

#[cfg(feature = "wasm32")]
fn init_wasm32(options: InitOptions) -> Result<(), Error> {
    use crate::{Format, Sink};
    use tracing_subscriber::layer::SubscriberExt;

    if WASM32_LOG_INIT.get().is_some() {
        return Ok(());
    }

    if options.sink == Sink::Journald {
        return Err(Error::InvalidConfiguration(
            "Sink::Journald is only available with the systemd feature".to_owned(),
        ));
    }

    console_error_panic_hook::set_once();

    let filter_layer = tracing_subscriber::EnvFilter::new(options.resolved_default_filter());
    let writer = tracing_web::MakeWebConsoleWriter::new();
    let timer = tracing_subscriber::fmt::time::UtcTime::rfc_3339();
    let subscriber = tracing_subscriber::registry().with(filter_layer);

    let set_result = match options.resolved_wasm_format() {
        Format::Json | Format::Auto => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_ansi(false)
                .with_timer(timer)
                .with_current_span(true)
                .with_span_list(false)
                .with_writer(writer);
            tracing::subscriber::set_global_default(subscriber.with(fmt_layer))
        }
        Format::Pretty => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_ansi(false)
                .with_timer(timer)
                .with_writer(writer);
            tracing::subscriber::set_global_default(subscriber.with(fmt_layer))
        }
        Format::Compact => {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_ansi(false)
                .with_timer(timer)
                .with_writer(writer);
            tracing::subscriber::set_global_default(subscriber.with(fmt_layer))
        }
    };

    if let Err(err) = set_result {
        return if options.idempotent { Ok(()) } else { Err(Error::from(err)) };
    }

    let _ = WASM32_LOG_INIT.set(());
    if let Some(name) = options.service_name.as_deref() {
        tracing::info!(service.name = %name, "Logging initialized");
    } else {
        tracing::info!("Logging initialized");
    }
    Ok(())
}

#[cfg(all(test, feature = "systemd"))]
mod tests {
    use super::*;
    use crate::{Format, Sink};

    #[test]
    fn resolve_sink_passes_through_explicit_choices() {
        assert_eq!(resolve_sink(Sink::Stdout, false), Sink::Stdout);
        assert_eq!(resolve_sink(Sink::Stderr, true), Sink::Stderr);
        assert_eq!(resolve_sink(Sink::Journald, true), Sink::Journald);
    }

    #[test]
    fn resolve_format_passes_through_explicit_choices() {
        assert_eq!(resolve_format(Format::Json, Sink::Stdout, true), Format::Json);
        assert_eq!(resolve_format(Format::Pretty, Sink::Journald, false), Format::Pretty);
        assert_eq!(resolve_format(Format::Compact, Sink::Auto, false), Format::Compact);
    }

    #[test]
    fn resolve_auto_falls_back_when_env_unset() {
        // Only assert the env-independent fallbacks when the override is absent,
        // so the test stays deterministic regardless of the ambient environment.
        if std::env::var("KAMU_LOG_SINK").is_err() {
            assert_eq!(resolve_sink(Sink::Auto, true), Sink::Stdout);
            assert_eq!(resolve_sink(Sink::Auto, false), Sink::Journald);
        }
        if std::env::var("KAMU_LOG_FORMAT").is_err() {
            assert_eq!(resolve_format(Format::Auto, Sink::Journald, false), Format::Compact);
            assert_eq!(resolve_format(Format::Auto, Sink::Stdout, true), Format::Pretty);
            assert_eq!(resolve_format(Format::Auto, Sink::Stderr, false), Format::Compact);
            assert_eq!(resolve_format(Format::Auto, Sink::Auto, false), Format::Compact);
        }
    }

    #[test]
    fn build_output_layer_constructs_console_layers() {
        // The Journald arm is environment-bound (needs the journald socket) and
        // is exercised separately; the console arms cover every format branch.
        for sink in [Sink::Stdout, Sink::Stderr] {
            for format in [Format::Json, Format::Pretty, Format::Compact, Format::Auto] {
                assert!(build_output_layer(sink, format, false).is_ok());
                assert!(build_output_layer(sink, format, true).is_ok());
            }
        }
    }
}
