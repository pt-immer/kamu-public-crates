//! OpenTelemetry OTLP exporter wiring.
//!
//! Enabled by the `with-otlp` feature. Adds a [`tracing_opentelemetry`]
//! layer to the subscriber stack that exports spans to an OTLP collector
//! over HTTP/protobuf.
//!
//! Export uses an in-process [`BatchSpanProcessor`] by default: spans are
//! buffered and flushed from a dedicated background OS thread, so export never
//! blocks the thread that closes a span and **no async runtime is required** by
//! this crate. Opt into synchronous inline export — handy for tests and
//! short-lived CLIs — with [`OtlpConfig::with_processor`] and
//! [`SpanProcessorMode::Simple`]. Batch buffering is tunable via
//! [`OtlpConfig::with_max_queue_size`], [`OtlpConfig::with_scheduled_delay`],
//! and [`OtlpConfig::with_max_export_batch_size`].
//!
//! The batch buffer drains on a periodic timer (~5 s by default), so a process
//! that exits abruptly can lose the last, unflushed batch. Call
//! [`shutdown_otlp`] (or [`flush_otlp`]) before exit to drain it.
//!
//! [`BatchSpanProcessor`]: opentelemetry_sdk::trace::BatchSpanProcessor

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider};

/// Span-processor strategy for the OTLP exporter.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpanProcessorMode {
    /// In-process batch export from a dedicated background OS thread (no async
    /// runtime). Export happens off the thread that closed the span. Default.
    #[default]
    Batch,
    /// Synchronous inline export on the closing thread — no background thread
    /// and no buffering, so every span is exported before the call returns.
    /// Deterministic; suited to tests and short-lived CLIs.
    Simple,
}

/// Configuration for the OTLP exporter.
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    pub(crate) endpoint: String,
    pub(crate) service_name: Option<String>,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) resource_attributes: Vec<(String, String)>,
    pub(crate) processor: SpanProcessorMode,
    pub(crate) max_queue_size: Option<usize>,
    pub(crate) scheduled_delay: Option<Duration>,
    pub(crate) max_export_batch_size: Option<usize>,
}

impl OtlpConfig {
    /// Endpoint for the OTLP/HTTP collector, e.g.
    /// `https://otel-collector.example.com:4318`.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: None,
            headers: HashMap::new(),
            resource_attributes: Vec::new(),
            processor: SpanProcessorMode::Batch,
            max_queue_size: None,
            scheduled_delay: None,
            max_export_batch_size: None,
        }
    }

    /// Set `service.name` on the resource. Most collectors require this for
    /// per-service routing/quotas.
    #[must_use]
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Add an HTTP header sent on every export request (e.g. auth tokens).
    #[must_use]
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add an arbitrary resource attribute (e.g. `deployment.environment`).
    #[must_use]
    pub fn with_resource_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.resource_attributes.push((key.into(), value.into()));
        self
    }

    /// Select the span-processor strategy. Defaults to
    /// [`SpanProcessorMode::Batch`].
    #[must_use]
    pub fn with_processor(mut self, mode: SpanProcessorMode) -> Self {
        self.processor = mode;
        self
    }

    /// Maximum number of spans buffered before new spans are dropped.
    ///
    /// Batch mode only — ignored under [`SpanProcessorMode::Simple`]. Defaults
    /// to the SDK default (2048) or the `OTEL_BSP_MAX_QUEUE_SIZE` env var.
    #[must_use]
    pub fn with_max_queue_size(mut self, max_queue_size: usize) -> Self {
        self.max_queue_size = Some(max_queue_size);
        self
    }

    /// Delay between automatic batch exports.
    ///
    /// Batch mode only — ignored under [`SpanProcessorMode::Simple`]. Defaults
    /// to the SDK default (5 s) or the `OTEL_BSP_SCHEDULE_DELAY` env var.
    #[must_use]
    pub fn with_scheduled_delay(mut self, scheduled_delay: Duration) -> Self {
        self.scheduled_delay = Some(scheduled_delay);
        self
    }

    /// Maximum number of spans per export request (the SDK clamps this to
    /// `max_queue_size`).
    ///
    /// Batch mode only — ignored under [`SpanProcessorMode::Simple`]. Defaults
    /// to the SDK default (512) or the `OTEL_BSP_MAX_EXPORT_BATCH_SIZE` env var.
    #[must_use]
    pub fn with_max_export_batch_size(mut self, max_export_batch_size: usize) -> Self {
        self.max_export_batch_size = Some(max_export_batch_size);
        self
    }
}

/// Build the OTLP `tracing` layer plus the owning [`SdkTracerProvider`].
///
/// The returned provider is a second owned handle (the layer's tracer holds its
/// own `Arc`); [`init`](crate::init_with) stows it via [`store_provider`] so
/// [`flush_otlp`] / [`shutdown_otlp`] can drain the batch later.
pub(crate) fn build_layer<S>(
    config: &OtlpConfig,
) -> Result<
    (tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>, SdkTracerProvider),
    crate::Error,
>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let mut builder = SpanExporter::builder().with_http().with_endpoint(&config.endpoint);

    if !config.headers.is_empty() {
        builder = builder.with_headers(config.headers.clone());
    }

    let exporter = builder.build().map_err(|e| crate::Error::OtlpInit(e.to_string()))?;

    let mut attrs: Vec<KeyValue> =
        config.resource_attributes.iter().map(|(k, v)| KeyValue::new(k.clone(), v.clone())).collect();
    if let Some(name) = config.service_name.as_deref() {
        attrs.push(KeyValue::new("service.name", name.to_owned()));
    }

    let resource = Resource::builder().with_attributes(attrs).build();
    let provider_builder = SdkTracerProvider::builder().with_resource(resource);

    let provider = match config.processor {
        SpanProcessorMode::Batch => {
            let mut batch_config = BatchConfigBuilder::default();
            if let Some(size) = config.max_queue_size {
                batch_config = batch_config.with_max_queue_size(size);
            }
            if let Some(delay) = config.scheduled_delay {
                batch_config = batch_config.with_scheduled_delay(delay);
            }
            if let Some(size) = config.max_export_batch_size {
                batch_config = batch_config.with_max_export_batch_size(size);
            }
            // The batch processor exports on its own OS thread, so no async
            // runtime is needed and the `reqwest-blocking` client is safe to
            // build even inside a tokio/actix runtime.
            let processor =
                BatchSpanProcessor::builder(exporter).with_batch_config(batch_config.build()).build();
            provider_builder.with_span_processor(processor).build()
        }
        SpanProcessorMode::Simple => provider_builder.with_simple_exporter(exporter).build(),
    };

    let tracer = provider.tracer("kamu-logging");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    Ok((layer, provider))
}

/// Process-global handle to the active OTLP tracer provider, retained so the
/// buffered batch can be drained after init. Set once per process, mirroring
/// the one-shot global subscriber.
static OTLP_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

/// Retain the provider built in [`build_layer`] for later flush/shutdown.
///
/// First write wins; later calls are ignored (matches the one-shot subscriber).
pub(crate) fn store_provider(provider: SdkTracerProvider) {
    let _ = OTLP_PROVIDER.set(provider);
}

/// Flush spans the batch processor has buffered, blocking until the export
/// completes or times out.
///
/// A no-op returning `Ok(())` when OTLP was never initialized.
///
/// # Errors
///
/// [`Error::OtlpInit`](crate::Error::OtlpInit) if the exporter reports a flush
/// failure.
pub fn flush_otlp() -> Result<(), crate::Error> {
    match OTLP_PROVIDER.get() {
        Some(provider) => provider.force_flush().map_err(|e| crate::Error::OtlpInit(e.to_string())),
        None => Ok(()),
    }
}

/// Drain and stop the OTLP exporter, flushing the final batch.
///
/// Call once before process exit (e.g. from a `SIGTERM` handler) so the last
/// buffered spans are not lost. Idempotent: a no-op returning `Ok(())` when OTLP
/// was never initialized or has already been shut down.
///
/// # Errors
///
/// [`Error::OtlpInit`](crate::Error::OtlpInit) if the exporter reports a
/// shutdown failure other than "already shut down".
pub fn shutdown_otlp() -> Result<(), crate::Error> {
    let Some(provider) = OTLP_PROVIDER.get() else {
        return Ok(());
    };
    match provider.shutdown() {
        Ok(()) | Err(opentelemetry_sdk::error::OTelSdkError::AlreadyShutdown) => Ok(()),
        Err(err) => Err(crate::Error::OtlpInit(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_batch_with_unset_knobs() {
        let cfg = OtlpConfig::new("http://127.0.0.1:4318");
        assert_eq!(cfg.processor, SpanProcessorMode::Batch);
        assert_eq!(cfg.max_queue_size, None);
        assert_eq!(cfg.scheduled_delay, None);
        assert_eq!(cfg.max_export_batch_size, None);
    }

    #[test]
    fn span_processor_mode_default_is_batch() {
        assert_eq!(SpanProcessorMode::default(), SpanProcessorMode::Batch);
    }

    #[test]
    fn with_processor_overrides_mode() {
        let cfg = OtlpConfig::new("http://127.0.0.1:4318").with_processor(SpanProcessorMode::Simple);
        assert_eq!(cfg.processor, SpanProcessorMode::Simple);
    }

    #[test]
    fn tuning_builders_set_fields() {
        let cfg = OtlpConfig::new("http://127.0.0.1:4318")
            .with_max_queue_size(4096)
            .with_scheduled_delay(Duration::from_secs(2))
            .with_max_export_batch_size(1024);
        assert_eq!(cfg.max_queue_size, Some(4096));
        assert_eq!(cfg.scheduled_delay, Some(Duration::from_secs(2)));
        assert_eq!(cfg.max_export_batch_size, Some(1024));
    }

    #[test]
    fn flush_and_shutdown_are_noops_without_init() {
        // No provider has been stored in this test binary, so both drain helpers
        // must succeed as no-ops rather than panic or error.
        assert!(flush_otlp().is_ok());
        assert!(shutdown_otlp().is_ok());
    }
}
