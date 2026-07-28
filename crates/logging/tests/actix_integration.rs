//! End-to-end Actix root-span enrichment and completion behavior.

#![cfg(all(feature = "systemd", feature = "with-actix-web"))]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use actix_web::http::header::HeaderValue;
use actix_web::{App, HttpResponse, error, test, web};
use kamu_logging::get_actix_web_logger;
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};

#[derive(Clone, Default)]
struct SpanCapture {
    spans: Arc<Mutex<HashMap<u64, CapturedSpan>>>,
}

#[derive(Clone, Default)]
struct CapturedSpan {
    fields: BTreeMap<String, String>,
    closed: bool,
}

impl SpanCapture {
    fn for_target(&self, target: &str) -> CapturedSpan {
        self.spans
            .lock()
            .expect("capture lock")
            .values()
            .find(|span| span.fields.get("http.target").is_some_and(|value| value == target))
            .cloned()
            .unwrap_or_else(|| panic!("missing HTTP span for {target}"))
    }
}

impl<S> Layer<S> for SpanCapture
where
    S: Subscriber,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, _context: Context<'_, S>) {
        if attributes.metadata().name() != "HTTP request" {
            return;
        }

        let mut captured = CapturedSpan::default();
        attributes.record(&mut FieldVisitor(&mut captured.fields));
        self.spans.lock().expect("capture lock").insert(id.into_u64(), captured);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, _context: Context<'_, S>) {
        let mut spans = self.spans.lock().expect("capture lock");
        if let Some(span) = spans.get_mut(&id.into_u64()) {
            values.record(&mut FieldVisitor(&mut span.fields));
        }
    }

    fn on_close(&self, id: Id, _context: Context<'_, S>) {
        if let Some(span) = self.spans.lock().expect("capture lock").get_mut(&id.into_u64()) {
            span.closed = true;
        }
    }
}

struct FieldVisitor<'a>(&'a mut BTreeMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), value.to_string());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

async fn created() -> HttpResponse {
    HttpResponse::Created().finish()
}

async fn failed() -> Result<HttpResponse, actix_web::Error> {
    Err(error::ErrorInternalServerError("integration-boom"))
}

#[actix_web::test]
async fn middleware_enriches_and_completes_real_request_spans() {
    let capture = SpanCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    let _subscriber_guard = tracing::subscriber::set_default(subscriber);

    let app = test::init_service(
        App::new()
            .wrap(get_actix_web_logger())
            .route("/precedence", web::get().to(created))
            .route("/bad-trace", web::get().to(created))
            .route("/non-ascii", web::get().to(created))
            .route("/error", web::get().to(failed)),
    )
    .await;

    let precedence = test::TestRequest::get()
        .uri("/precedence")
        .insert_header(("x-request-id", "request-first"))
        .append_header(("x-request-id", "request-repeated"))
        .insert_header(("x-correlation-id", "correlation-second"))
        .insert_header(("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"))
        .to_request();
    let response = test::call_service(&app, precedence).await;
    assert_eq!(response.status(), actix_web::http::StatusCode::CREATED);
    test::read_body(response).await;

    let bad_trace = test::TestRequest::get()
        .uri("/bad-trace")
        .insert_header(("traceparent", "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01"))
        .to_request();
    let response = test::call_service(&app, bad_trace).await;
    test::read_body(response).await;

    let non_ascii = test::TestRequest::get()
        .uri("/non-ascii")
        .insert_header(("x-request-id", HeaderValue::from_bytes(&[0x80]).expect("HTTP obs-text value")))
        .insert_header(("x-correlation-id", "ascii-fallback"))
        .to_request();
    let response = test::call_service(&app, non_ascii).await;
    test::read_body(response).await;

    let error_response = test::call_service(&app, test::TestRequest::get().uri("/error").to_request()).await;
    assert_eq!(error_response.status(), actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    test::read_body(error_response).await;

    let precedence = capture.for_target("/precedence");
    assert_eq!(precedence.fields.get("correlation_id").map(String::as_str), Some("request-first"));
    assert_eq!(precedence.fields.get("http.status_code").map(String::as_str), Some("201"));
    assert!(precedence.closed);

    let bad_trace = capture.for_target("/bad-trace");
    assert!(!bad_trace.fields.contains_key("correlation_id"));
    assert!(bad_trace.closed);

    let non_ascii = capture.for_target("/non-ascii");
    assert_eq!(non_ascii.fields.get("correlation_id").map(String::as_str), Some("ascii-fallback"));
    assert!(non_ascii.closed);

    let error = capture.for_target("/error");
    assert_eq!(error.fields.get("http.status_code").map(String::as_str), Some("500"));
    assert!(
        error.fields.get("exception.message").is_some_and(|message| message.contains("integration-boom"))
    );
    assert_eq!(error.fields.get("otel.status_code").map(String::as_str), Some("ERROR"));
    assert!(error.closed);
}
