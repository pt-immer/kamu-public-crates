//! OpenTelemetry OTLP exporter wiring.
//!
//! Enabled by the `with-otlp` feature. Adds a [`tracing_opentelemetry`]
//! layer to the subscriber stack that exports spans to an OTLP collector
//! over HTTP/protobuf.
//!
//! Uses a synchronous `SimpleSpanProcessor` so no async runtime is required
//! by this crate. High-throughput services should replace the layer with a
//! batch processor configured against their runtime.

use std::collections::HashMap;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// Configuration for the OTLP exporter.
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    pub(crate) endpoint: String,
    pub(crate) service_name: Option<String>,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) resource_attributes: Vec<(String, String)>,
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
}

pub(crate) fn build_layer<S>(
    config: &OtlpConfig,
) -> Result<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>, crate::Error>
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
    let provider =
        SdkTracerProvider::builder().with_simple_exporter(exporter).with_resource(resource).build();

    let tracer = provider.tracer("kamu-logging");
    Ok(tracing_opentelemetry::layer().with_tracer(tracer))
}
