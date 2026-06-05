//! Configuration for [`init_with`](crate::init_with).

/// Output format for the fmt layer.
///
/// `Auto` is replaced at init time by [`Format::Pretty`] when the chosen sink
/// is a TTY and [`Format::Compact`] otherwise. Set the `KAMU_LOG_FORMAT`
/// environment variable (`auto`, `compact`, `pretty`, `json`) to override
/// without code changes. On wasm32, `Auto` resolves to [`Format::Json`] for
/// Cloudflare Workers Logs-friendly console output, and [`Format::Pretty`]
/// falls back to non-ANSI compact output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// Resolved at init time based on sink + env var.
    #[default]
    Auto,
    /// Single-line, machine-readable text format.
    Compact,
    /// Multi-line, human-readable text format with ANSI colors.
    Pretty,
    /// Line-delimited JSON. Required for log aggregators (Vector, Promtail,
    /// Datadog Agent, Fluent Bit).
    Json,
}

impl Format {
    /// Parse from an env-var value. Unknown values resolve to `Auto`.
    #[must_use]
    pub fn from_env_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "compact" => Self::Compact,
            "pretty" => Self::Pretty,
            "json" => Self::Json,
            _ => Self::Auto,
        }
    }
}

/// Where to write log events.
///
/// `Auto` (default) emits to the console when stdout is a TTY and to journald
/// otherwise. Set `KAMU_LOG_SINK` (`auto`, `stdout`, `stderr`, `journald`) to
/// override without code changes. `Journald` is rejected on targets without
/// the `systemd` feature. On wasm32, `Auto`, `Stdout`, and `Stderr` all map to
/// the JavaScript console.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Sink {
    /// TTY → console, non-TTY → journald (on systemd) or stderr (otherwise).
    #[default]
    Auto,
    /// Write to stdout.
    Stdout,
    /// Write to stderr.
    Stderr,
    /// Write to the systemd journal. Requires the `systemd` feature.
    Journald,
}

impl Sink {
    /// Parse from an env-var value. Unknown values resolve to `Auto`.
    #[must_use]
    pub fn from_env_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "stdout" => Self::Stdout,
            "stderr" => Self::Stderr,
            "journald" => Self::Journald,
            _ => Self::Auto,
        }
    }
}

/// Configuration for [`init_with`](crate::init_with).
///
/// Constructed via [`InitOptions::default`] and the `with_*` builder methods.
/// Each method consumes `self` and returns `Self` to support chaining:
///
/// ```no_run
/// use kamu_logging::{init_with, Format, InitOptions, Sink};
///
/// init_with(
///     InitOptions::default()
///         .with_service_name("my-service")
///         .with_format(Format::Json)
///         .with_sink(Sink::Stdout),
/// )?;
/// # Ok::<(), kamu_logging::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct InitOptions {
    pub(crate) service_name: Option<String>,
    pub(crate) default_filter: Option<String>,
    pub(crate) env_var: Option<String>,
    pub(crate) format: Format,
    pub(crate) sink: Sink,
    pub(crate) idempotent: bool,
    #[cfg(feature = "with-otlp")]
    pub(crate) otlp: Option<crate::otlp::OtlpConfig>,
}

impl InitOptions {
    /// Attach a `service.name` field to every event. Used by log aggregators
    /// to route logs across services.
    #[must_use]
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Default filter directive when the env var is unset. Defaults to
    /// `"debug"` in debug builds and `"info"` in release builds.
    #[must_use]
    pub fn with_default_filter(mut self, filter: impl Into<String>) -> Self {
        self.default_filter = Some(filter.into());
        self
    }

    /// Environment variable read for the filter directive. Defaults to
    /// `"RUST_LOG"`. Useful for per-binary triggers like `"KKP_LOG"` to avoid
    /// collisions with other tools' `RUST_LOG` settings.
    #[must_use]
    pub fn with_env_var(mut self, var: impl Into<String>) -> Self {
        self.env_var = Some(var.into());
        self
    }

    /// Output format. See [`Format`].
    #[must_use]
    pub fn with_format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Output sink. See [`Sink`].
    #[must_use]
    pub fn with_sink(mut self, sink: Sink) -> Self {
        self.sink = sink;
        self
    }

    /// When `true`, a second [`init_with`](crate::init_with) call returns
    /// `Ok(())` instead of [`Error::TracingGlobal`](crate::Error::TracingGlobal).
    ///
    /// Default is `false` so library double-init surfaces as an error. Set
    /// `true` from test harnesses and embedded CLI runs where re-init is
    /// expected.
    #[must_use]
    pub fn idempotent(mut self, enabled: bool) -> Self {
        self.idempotent = enabled;
        self
    }

    /// Attach an OpenTelemetry OTLP exporter layer. Requires the `with-otlp`
    /// feature.
    #[cfg(feature = "with-otlp")]
    #[must_use]
    pub fn with_otlp(mut self, config: crate::otlp::OtlpConfig) -> Self {
        self.otlp = Some(config);
        self
    }

    #[cfg(feature = "systemd")]
    pub(crate) fn resolved_env_var(&self) -> &str {
        self.env_var.as_deref().unwrap_or("RUST_LOG")
    }

    #[cfg(feature = "systemd")]
    pub(crate) fn resolved_default_filter(&self) -> &str {
        if let Some(filter) = self.default_filter.as_deref() {
            return filter;
        }
        if cfg!(debug_assertions) { "debug" } else { "info" }
    }

    #[cfg(feature = "wasm32")]
    pub(crate) fn resolved_default_filter(&self) -> &str {
        if let Some(filter) = self.default_filter.as_deref() {
            return filter;
        }
        if cfg!(debug_assertions) { "debug" } else { "info" }
    }

    #[cfg(feature = "wasm32")]
    pub(crate) fn resolved_wasm_format(&self) -> Format {
        match self.format {
            Format::Auto => Format::Json,
            format => format,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_every_field() {
        let opts = InitOptions::default()
            .with_service_name("svc")
            .with_default_filter("warn")
            .with_env_var("KKP_LOG")
            .with_format(Format::Json)
            .with_sink(Sink::Stdout)
            .idempotent(true);
        assert_eq!(opts.service_name.as_deref(), Some("svc"));
        assert_eq!(opts.default_filter.as_deref(), Some("warn"));
        assert_eq!(opts.env_var.as_deref(), Some("KKP_LOG"));
        assert_eq!(opts.format, Format::Json);
        assert_eq!(opts.sink, Sink::Stdout);
        assert!(opts.idempotent);
    }

    #[cfg(feature = "systemd")]
    #[test]
    fn resolved_helpers_apply_defaults_then_overrides() {
        let default = InitOptions::default();
        assert_eq!(default.resolved_env_var(), "RUST_LOG");
        let expected = if cfg!(debug_assertions) { "debug" } else { "info" };
        assert_eq!(default.resolved_default_filter(), expected);

        let custom = InitOptions::default().with_env_var("KKP_LOG").with_default_filter("trace");
        assert_eq!(custom.resolved_env_var(), "KKP_LOG");
        assert_eq!(custom.resolved_default_filter(), "trace");
    }
}
