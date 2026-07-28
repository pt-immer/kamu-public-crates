//! Subscriber construction.

use std::sync::{Mutex, OnceLock};

#[cfg(feature = "systemd")]
use std::sync::atomic::{AtomicU8, Ordering};

use crate::{Error, InitOptions};

static INSTALL_GUARD: Mutex<()> = Mutex::new(());
static INSTALLED: OnceLock<Installed> = OnceLock::new();

struct Installed {
    #[cfg(feature = "systemd")]
    log_bridge: AtomicU8,
    #[cfg(feature = "with-otlp")]
    otlp_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

#[cfg(feature = "systemd")]
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogBridge {
    Installing = 0,
    Ours = 1,
    Foreign = 2,
}

impl Installed {
    fn repeat_result(&self, idempotent: bool) -> Result<(), Error> {
        #[cfg(feature = "systemd")]
        match self.log_bridge.load(Ordering::Acquire) {
            value if value == LogBridge::Ours as u8 => {}
            value if value == LogBridge::Foreign as u8 => {
                return Err(Error::ForeignGlobalLogger);
            }
            _ => return Err(Error::InstallationIncomplete),
        }

        if idempotent { Ok(()) } else { Err(Error::AlreadyInitialized) }
    }
}

/// Initialize the global tracing subscriber with default options.
///
/// Equivalent to `init_with(InitOptions::default())`.
///
/// # Errors
///
/// Returns [`Error::AlreadyInitialized`] if this crate already installed the
/// subscriber. A foreign owner returns a distinct ownership error.
pub fn init() -> Result<(), Error> {
    init_with(InitOptions::default())
}

/// Initialize the global tracing subscriber, returning `Ok(())` if this crate
/// already installed it.
///
/// Equivalent to `init_with(InitOptions::default().idempotent(true))`.
///
/// # Errors
///
/// A subscriber or `log` facade installed by another crate is reported as
/// [`Error::ForeignGlobalSubscriber`] or [`Error::ForeignGlobalLogger`].
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
/// **Build-specific behavior.** The `KAMU_LOG_*` overrides above, and the
/// `RUST_LOG` / [`InitOptions::with_env_var`] filter, apply on the **systemd**
/// build. The `wasm32` build has no process environment: the filter is taken
/// solely from `default_filter` (plumb your platform's env in yourself, e.g.
/// via [`InitOptions::with_default_filter`]), and the `KAMU_LOG_*` /
/// `with_env_var` env sources are not consulted. The first-init-wins
/// idempotence and the `AlreadyInitialized` error below apply to both builds.
///
/// # Errors
///
/// - [`Error::AlreadyInitialized`] if `idempotent` is false and this crate
///   already installed the subscriber.
/// - [`Error::InvalidConfiguration`] if the selected target cannot honor an
///   option (for example `Sink::Journald` on wasm32).
/// - [`Error::ForeignGlobalSubscriber`] when another subscriber owns the
///   process-global slot.
/// - [`Error::ForeignGlobalLogger`] when another logger owns the `log` facade.
/// - [`Error::InstallationIncomplete`] if an earlier install panicked after
///   claiming the subscriber but before committing the `log` bridge.
/// - [`Error::IO`] if the journald socket is unavailable.
/// - `Error::OtlpInit` (with-otlp feature) if the exporter cannot be built.
///
/// On the systemd build, [`Error::ForeignGlobalLogger`] is discovered only
/// after this crate commits the tracing subscriber (and any OTLP provider).
/// That owned subscriber remains installed; retrying reports the same logger
/// conflict. This order avoids claiming the `log` facade and then failing to
/// install the tracing subscriber.
pub fn init_with(options: InitOptions) -> Result<(), Error> {
    let _install_guard = INSTALL_GUARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(installed) = INSTALLED.get() {
        return installed.repeat_result(options.idempotent);
    }

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

    let env_var = options.resolved_env_var().to_owned();
    let default_filter = options.resolved_default_filter().to_owned();
    let filter_layer = resolve_filter(&env_var, &default_filter)?;

    let sink_override = if options.sink == crate::Sink::Auto { read_env("KAMU_LOG_SINK")? } else { None };
    let format_override =
        if options.format == crate::Format::Auto { read_env("KAMU_LOG_FORMAT")? } else { None };
    let effective_sink = resolve_sink(options.sink, sink_override.as_deref())?;
    let is_tty = selected_stream_is_tty(effective_sink);
    let effective_format =
        resolve_format(options.format, effective_sink, is_tty, format_override.as_deref())?;

    let output_layer = build_output_layer(effective_sink, effective_format, is_tty)?;

    #[cfg(feature = "with-otlp")]
    let (otlp_layer, otlp_provider) = match options.otlp.as_ref() {
        Some(cfg) => {
            let mut cfg = cfg.clone();
            if cfg.service_name.is_none() {
                cfg.service_name.clone_from(&options.service_name);
            }
            let (layer, provider) = crate::otlp::build_layer(&cfg)?;
            (Some(layer), Some(provider))
        }
        None => (None, None),
    };

    let subscriber = tracing_subscriber::registry().with(filter_layer).with(output_layer);

    #[cfg(feature = "with-otlp")]
    let subscriber = subscriber.with(otlp_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        #[cfg(feature = "with-otlp")]
        shutdown_uncommitted_provider(otlp_provider.as_ref());
        return Err(Error::ForeignGlobalSubscriber);
    }

    let installed = Installed {
        log_bridge: AtomicU8::new(LogBridge::Installing as u8),
        #[cfg(feature = "with-otlp")]
        otlp_provider,
    };
    if INSTALLED.set(installed).is_err() {
        unreachable!("installation mutex permits one committed state");
    }

    let log_bridge =
        if tracing_log::LogTracer::init().is_ok() { LogBridge::Ours } else { LogBridge::Foreign };
    INSTALLED
        .get()
        .expect("subscriber commit publishes installed state")
        .log_bridge
        .store(log_bridge as u8, Ordering::Release);

    if let Some(name) = options.service_name.as_deref() {
        tracing::info!(service.name = %name, "Logging initialized");
    } else {
        tracing::info!("Logging initialized");
    }
    if log_bridge == LogBridge::Ours { Ok(()) } else { Err(Error::ForeignGlobalLogger) }
}

#[cfg(feature = "systemd")]
fn resolve_filter(env_var: &str, default_filter: &str) -> Result<tracing_subscriber::EnvFilter, Error> {
    let directive = match std::env::var(env_var) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => default_filter.to_owned(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Error::InvalidEnvironmentValue {
                variable: env_var.to_owned(),
                expected: "valid Unicode containing a tracing filter directive",
            });
        }
    };

    tracing_subscriber::EnvFilter::try_new(directive).map_err(|_| {
        if std::env::var_os(env_var).is_some() {
            Error::InvalidEnvironmentValue {
                variable: env_var.to_owned(),
                expected: "a tracing filter directive",
            }
        } else {
            Error::InvalidConfiguration("default filter is not a tracing filter directive".to_owned())
        }
    })
}

#[cfg(feature = "systemd")]
fn read_env(name: &'static str) -> Result<Option<String>, Error> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::InvalidEnvironmentValue { variable: name.to_owned(), expected: "valid Unicode" })
        }
    }
}

#[cfg(feature = "systemd")]
fn resolve_format(
    format: crate::Format,
    sink: crate::Sink,
    is_tty: bool,
    env_value: Option<&str>,
) -> Result<crate::Format, Error> {
    use crate::{Format, Sink};

    let mut effective = format;
    if effective == Format::Auto
        && let Some(value) = env_value
    {
        effective = Format::from_env_value(value).map_err(|_| Error::InvalidEnvironmentValue {
            variable: "KAMU_LOG_FORMAT".to_owned(),
            expected: "one of: auto, compact, pretty, json",
        })?;
    }
    if effective != Format::Auto {
        return Ok(effective);
    }
    Ok(match sink {
        Sink::Journald => Format::Compact,
        Sink::Stdout | Sink::Stderr if is_tty => Format::Pretty,
        Sink::Stdout | Sink::Stderr => Format::Compact,
        Sink::Auto => Format::Compact,
    })
}

#[cfg(feature = "systemd")]
fn resolve_sink(sink: crate::Sink, env_value: Option<&str>) -> Result<crate::Sink, Error> {
    use crate::Sink;

    let mut effective = sink;
    if effective == Sink::Auto
        && let Some(value) = env_value
    {
        effective = Sink::from_env_value(value).map_err(|_| Error::InvalidEnvironmentValue {
            variable: "KAMU_LOG_SINK".to_owned(),
            expected: "one of: auto, stdout, stderr, journald",
        })?;
    }
    Ok(if effective == Sink::Auto { Sink::Stderr } else { effective })
}

#[cfg(feature = "systemd")]
fn selected_stream_is_tty(sink: crate::Sink) -> bool {
    match sink {
        crate::Sink::Stdout => console::Term::stdout().is_term(),
        crate::Sink::Stderr => console::Term::stderr().is_term(),
        crate::Sink::Auto | crate::Sink::Journald => false,
    }
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
        Sink::Stdout => fmt::writer::BoxMakeWriter::new(std::io::stdout),
        Sink::Auto => {
            return Err(Error::InvalidConfiguration(
                "Sink::Auto must be resolved before building the output layer".to_owned(),
            ));
        }
        Sink::Journald => unreachable!("journald returned above"),
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
        Format::Compact => Box::new(
            fmt::layer()
                .with_writer(writer)
                .with_span_events(span_events)
                .with_ansi(with_ansi)
                .with_line_number(true)
                .with_thread_ids(true)
                .compact()
                .boxed(),
        ),
        Format::Auto => {
            return Err(Error::InvalidConfiguration(
                "Format::Auto must be resolved before building the output layer".to_owned(),
            ));
        }
    };

    Ok(layer)
}

#[cfg(feature = "wasm32")]
fn init_wasm32(options: InitOptions) -> Result<(), Error> {
    use crate::{Format, Sink};
    use tracing_subscriber::layer::SubscriberExt;

    if options.sink == Sink::Journald {
        return Err(Error::InvalidConfiguration(
            "Sink::Journald is only available with the systemd feature".to_owned(),
        ));
    }

    let filter_layer =
        tracing_subscriber::EnvFilter::try_new(options.resolved_default_filter()).map_err(|_| {
            Error::InvalidConfiguration("default filter is not a tracing filter directive".to_owned())
        })?;
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

    if set_result.is_err() {
        return Err(Error::ForeignGlobalSubscriber);
    }

    if INSTALLED.set(Installed {}).is_err() {
        unreachable!("installation mutex permits one committed state");
    }
    console_error_panic_hook::set_once();
    if let Some(name) = options.service_name.as_deref() {
        tracing::info!(service.name = %name, "Logging initialized");
    } else {
        tracing::info!("Logging initialized");
    }
    Ok(())
}

#[cfg(feature = "with-otlp")]
fn shutdown_uncommitted_provider(provider: Option<&opentelemetry_sdk::trace::SdkTracerProvider>) {
    if let Some(provider) = provider {
        let _ = provider.shutdown();
    }
}

#[cfg(feature = "with-otlp")]
pub(crate) fn otlp_provider() -> Option<&'static opentelemetry_sdk::trace::SdkTracerProvider> {
    INSTALLED.get().and_then(|installed| installed.otlp_provider.as_ref())
}

#[cfg(all(test, feature = "systemd"))]
mod tests {
    use super::*;
    use crate::{Format, Sink};

    #[test]
    fn installing_state_is_not_mistaken_for_idempotent_success() {
        let installed = Installed {
            log_bridge: AtomicU8::new(LogBridge::Installing as u8),
            #[cfg(feature = "with-otlp")]
            otlp_provider: None,
        };
        assert!(matches!(installed.repeat_result(true), Err(Error::InstallationIncomplete)));
    }

    #[test]
    fn resolve_sink_passes_through_explicit_choices() {
        assert_eq!(resolve_sink(Sink::Stdout, Some("journald")).expect("explicit sink"), Sink::Stdout);
        assert_eq!(resolve_sink(Sink::Stderr, Some("stdout")).expect("explicit sink"), Sink::Stderr);
        assert_eq!(resolve_sink(Sink::Journald, Some("stderr")).expect("explicit sink"), Sink::Journald);
    }

    #[test]
    fn resolve_format_passes_through_explicit_choices() {
        assert_eq!(
            resolve_format(Format::Json, Sink::Stdout, true, Some("compact")).expect("explicit format"),
            Format::Json
        );
        assert_eq!(
            resolve_format(Format::Pretty, Sink::Journald, false, None).expect("explicit format"),
            Format::Pretty
        );
        assert_eq!(
            resolve_format(Format::Compact, Sink::Stderr, false, None).expect("explicit format"),
            Format::Compact
        );
    }

    #[test]
    fn resolve_auto_has_portable_fallbacks() {
        assert_eq!(resolve_sink(Sink::Auto, None).expect("default sink"), Sink::Stderr);
        assert_eq!(resolve_sink(Sink::Auto, Some("auto")).expect("auto override"), Sink::Stderr);
        assert_eq!(resolve_sink(Sink::Auto, Some("journald")).expect("journald override"), Sink::Journald);
        assert_eq!(
            resolve_format(Format::Auto, Sink::Journald, false, None).expect("journald format"),
            Format::Compact
        );
        assert_eq!(
            resolve_format(Format::Auto, Sink::Stdout, true, None).expect("tty format"),
            Format::Pretty
        );
        assert_eq!(
            resolve_format(Format::Auto, Sink::Stderr, false, None).expect("non-tty format"),
            Format::Compact
        );
    }

    #[test]
    fn resolve_auto_rejects_unknown_environment_values() {
        assert!(matches!(
            resolve_sink(Sink::Auto, Some("console")),
            Err(Error::InvalidEnvironmentValue { variable, .. }) if variable == "KAMU_LOG_SINK"
        ));
        assert!(matches!(
            resolve_format(Format::Auto, Sink::Stderr, false, Some("structured")),
            Err(Error::InvalidEnvironmentValue { variable, .. }) if variable == "KAMU_LOG_FORMAT"
        ));
    }

    #[test]
    fn build_output_layer_constructs_console_layers() {
        // The Journald arm is environment-bound (needs the journald socket) and
        // is exercised separately; the console arms cover every format branch.
        for sink in [Sink::Stdout, Sink::Stderr] {
            for format in [Format::Json, Format::Pretty, Format::Compact] {
                assert!(build_output_layer(sink, format, false).is_ok());
                assert!(build_output_layer(sink, format, true).is_ok());
            }
        }
        assert!(build_output_layer(Sink::Stderr, Format::Auto, false).is_err());
        assert!(build_output_layer(Sink::Auto, Format::Compact, false).is_err());
    }
}
