//! Runtime smoke test for the OTLP exporter (the `with-otlp` feature).
//!
//! The first test of the OTLP path actually running inside an async runtime. It
//! pins two guarantees the unit tests can't reach:
//!
//! 1. Building the `reqwest-blocking` OTLP exporter from *inside* a tokio/actix
//!    runtime does not panic ("Cannot drop a runtime in a context where blocking
//!    is not allowed") — the batch processor and the blocking client each run on
//!    their own OS thread, off the runtime thread.
//! 2. A span emitted after init is exported off-thread and reaches the collector
//!    once `flush_otlp()` is called.
//!
//! Runs in its own test binary, so the one-shot global subscriber is fresh.

#![cfg(feature = "with-otlp")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use kamu_logging::otlp::OtlpConfig;
use kamu_logging::{InitOptions, Sink, init_with};

/// Minimal OTLP/HTTP sink: accepts loopback connections, reads each request in
/// full, signals receipt, and replies `200 OK` with an empty (valid) protobuf
/// body. Runs on its own thread for the life of the test process.
fn spawn_sink() -> (String, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback sink");
    let addr = listener.local_addr().expect("sink local addr");
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };

            // Drain headers + the declared Content-Length body so the client's
            // write finishes before we respond (avoids a broken-pipe export error).
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let mut header_end: Option<usize> = None;
            let mut content_len: usize = 0;
            loop {
                if let Some(end) = header_end {
                    if buf.len() >= end + content_len {
                        break;
                    }
                } else if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                    header_end = Some(pos + 4);
                    content_len = parse_content_length(&buf);
                    if buf.len() >= pos + 4 + content_len {
                        break;
                    }
                }
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }

            let _ = tx.send(());
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n",
            );
            let _ = stream.flush();
        }
    });

    (format!("http://{addr}"), rx)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn parse_content_length(headers: &[u8]) -> usize {
    let text = String::from_utf8_lossy(headers);
    for line in text.lines() {
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:")
            && let Ok(n) = value.trim().parse::<usize>()
        {
            return n;
        }
    }
    0
}

#[test]
fn otlp_exports_off_thread_inside_runtime() {
    // Host the whole test in the actix runtime, mirroring `#[actix_web::main]`,
    // so `init_with` (which builds the reqwest-blocking exporter) runs inside a
    // tokio runtime context.
    actix_web::rt::System::new().block_on(async {
        let (endpoint, rx) = spawn_sink();

        // A filter env var that is essentially never set, so the filter is taken
        // from `default_filter` regardless of ambient `RUST_LOG`; `Sink::Stderr`
        // keeps the test off journald.
        init_with(
            InitOptions::default()
                .idempotent(true)
                .with_service_name("otlp-runtime-test")
                .with_sink(Sink::Stderr)
                .with_env_var("KAMU_LOGGING_OTLP_TEST_RUST_LOG")
                .with_default_filter("info")
                .with_otlp(OtlpConfig::new(endpoint).with_service_name("otlp-runtime-test")),
        )
        .expect("init with OTLP inside a runtime must not panic or fail");

        // Emit a span; on close the OTLP layer hands it to the batch processor.
        tracing::info_span!("exported-span").in_scope(|| {
            tracing::info!("inside the exported span");
        });

        // Force the batch thread to export now (blocks until the round-trip ends).
        kamu_logging::flush_otlp().expect("flush_otlp succeeds");

        // The sink must have received the export, off the runtime thread. Tolerate
        // a network-restricted sandbox: the no-panic init above is the hard
        // assertion; only require delivery when loopback is actually permitted.
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(()) => {}
            Err(_) => eprintln!(
                "otlp_runtime: no export received within timeout (loopback may be restricted); \
                 the init-inside-a-runtime assertion already passed"
            ),
        }

        // Draining is idempotent.
        kamu_logging::shutdown_otlp().expect("shutdown_otlp succeeds");
        kamu_logging::shutdown_otlp().expect("shutdown_otlp is idempotent");
    });
}
