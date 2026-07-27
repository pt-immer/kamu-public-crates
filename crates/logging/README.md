# kamu-logging

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Opinionated `tracing` setup for PT IMMER services. One-line init for the
zero-config path; a builder for everything else (JSON output, custom env
vars, OTLP export, correlation ids).

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

## Install

```toml
[dependencies]
kamu-logging = "1"
```

MSRV: **Rust 1.94** (edition 2024).

## Features

| Feature          | Default | What it enables                                                       |
|------------------|:-------:|-----------------------------------------------------------------------|
| `systemd`        |   yes   | TTY-aware console + journald sink, `RUST_LOG`, `log` → `tracing` bridge |
| `with-actix-web` |   yes   | Correlation-enriched Actix Web middleware                              |
| `with-otlp`      |   no    | OpenTelemetry OTLP exporter (HTTP/protobuf)                           |
| `wasm32`         |   no    | Cloudflare Worker / web console + panic hook (mutually exclusive)     |

## Quickstart

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    kamu_logging::init()?;
    kamu_logging::info!("hello");
    Ok(())
}
```

`init()` picks a sensible default for the target: pretty TTY output when
stdout is a terminal, journald when not, and `RUST_LOG` for filtering.

## Configuration

For anything beyond the default path, build an `InitOptions`:

```rust
use kamu_logging::{Format, InitOptions, Sink, init_with};

init_with(
    InitOptions::default()
        .with_service_name("my-service")
        .with_default_filter("info,my_service=debug")
        .with_env_var("MY_SERVICE_LOG")
        .with_format(Format::Json)
        .with_sink(Sink::Stdout),
)?;
```

Builder methods (all consume `self`, all return `Self`):

| Method                   | Purpose                                                            |
|--------------------------|--------------------------------------------------------------------|
| `with_service_name(n)`   | Attach `service.name` to the startup event + OTLP `Resource`       |
| `with_default_filter(f)` | Filter directive used when the env var is unset                    |
| `with_env_var(v)`        | Env var read for the filter (default `RUST_LOG`)                   |
| `with_format(f)`         | `Auto` / `Compact` / `Pretty` / `Json`                             |
| `with_sink(s)`           | `Auto` / `Stdout` / `Stderr` / `Journald`                          |
| `idempotent(true)`       | Treat duplicate init as `Ok(())` (test harnesses, embedded runs)   |
| `with_otlp(cfg)`         | (with-otlp) Add an OTLP exporter layer                             |

### Env-var triggers (no code change)

| Variable           | Values                                  | Effect                                  |
|--------------------|-----------------------------------------|-----------------------------------------|
| `RUST_LOG`         | tracing-subscriber directive            | Filter directive (overridable per init) |
| `KAMU_LOG_FORMAT`  | `auto`, `compact`, `pretty`, `json`     | Sets `Format` when the option is `Auto` |
| `KAMU_LOG_SINK`    | `auto`, `stdout`, `stderr`, `journald`  | Sets `Sink` when the option is `Auto`   |

## JSON output for log aggregators

```rust
use kamu_logging::{Format, InitOptions, Sink, init_with};

init_with(
    InitOptions::default()
        .with_format(Format::Json)
        .with_sink(Sink::Stdout),
)?;
```

Or set `KAMU_LOG_FORMAT=json KAMU_LOG_SINK=stdout` at the process level
for zero-code adoption. Each event is one line of JSON suitable for
Vector, Promtail, Fluent Bit, or the Datadog Agent.

## Actix Web

```rust
use actix_web::{App, HttpServer};
use kamu_logging::get_actix_web_logger;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    kamu_logging::init().expect("init logging");
    HttpServer::new(|| App::new().wrap(get_actix_web_logger()))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
```

`get_actix_web_logger()` uses an `EnrichedRootSpanBuilder` that adds a
`correlation_id` field to the root span by extracting (in order):
`X-Request-ID`, `X-Correlation-ID`, `traceparent`. For a custom builder,
use `get_actix_web_logger_with::<MyBuilder>()`.

## Correlation outside HTTP

For queue consumers, scheduled tasks, or any non-HTTP entry point:

```rust
use kamu_logging::correlation::{with_id, extract_from_headers, DEFAULT_HEADER_CHAIN};

with_id("job-42", || {
    kamu_logging::info!("processing job");
});
```

The header-chain extractor is reusable for any framework — pass a closure
that fetches a header by name:

```rust
let id = extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, |h, name| {
    h.get(name).cloned()
});
```

## OTLP export

Enable the `with-otlp` feature and attach an `OtlpConfig`:

```rust
use kamu_logging::{InitOptions, init_with, otlp::OtlpConfig};

init_with(
    InitOptions::default()
        .with_service_name("checkout-api")
        .with_otlp(
            OtlpConfig::new("https://otel-collector.example.com:4318")
                .with_service_name("checkout-api")
                .with_header("authorization", "Bearer …")
                .with_resource_attribute("deployment.environment", "production"),
        ),
)?;
```

Export uses an in-process `BatchSpanProcessor` by default: spans are buffered
and flushed from a dedicated background OS thread, so export never blocks the
thread that closed the span and **no async runtime is required**. Tune the
buffer with `with_max_queue_size`, `with_scheduled_delay`, and
`with_max_export_batch_size`.

The batch buffer drains on a periodic timer (~5 s by default), so a process
that exits abruptly can lose the last, unflushed batch. Call `shutdown_otlp()`
(or `flush_otlp()`) before exit — e.g. from your `SIGTERM` handler — to drain
it:

```rust
// after the server stops accepting work, before the process exits:
kamu_logging::shutdown_otlp()?;
```

For deterministic synchronous export (handy in tests or short-lived CLIs), opt
back into the inline processor with
`.with_processor(SpanProcessorMode::Simple)`.

## WASM

```toml
[dependencies]
kamu-logging = { version = "1", default-features = false, features = ["wasm32"] }
```

`init()` on wasm32 installs `console_error_panic_hook` and a
`tracing-subscriber` console writer suitable for Cloudflare Workers Logs.
`Format::Auto` resolves to JSON, while `Sink::Auto`, `Sink::Stdout`, and
`Sink::Stderr` all write to the JavaScript console. The function is
idempotent on this target by design; subsequent calls are no-ops.

## Cloudflare Workers

Use `workers-rs` as usual, disable default features, and enable `wasm32`:

```toml
[dependencies]
kamu-logging = { version = "1", default-features = false, features = ["wasm32"] }
worker = "0.8"
```

The repository includes a standalone Worker app in
`examples/cloudflare-worker/`. For setup, Wrangler config, Workers Logs, and
correlation-id examples, see [the Cloudflare Workers guide](docs/CLOUDFLARE_WORKERS.md).

## Idempotence

- `init()` returns `Err(Error::AlreadyInitialized)` on a second call.
  Surfaces library double-init as a bug.
- `init_or_skip()` returns `Ok(())` on a second call. Use from test
  harnesses and embedded CLI runs.
- `InitOptions::idempotent(true)` does the same thing via the builder.

## Re-exported `tracing` items

So you can avoid a separate `tracing` import for the basics:

```rust
use kamu_logging::{debug, info, warn, error, instrument, span, Level, Span};
```

## Troubleshooting

| Symptom                                        | Fix                                                                    |
|------------------------------------------------|------------------------------------------------------------------------|
| No logs in container                           | Set `KAMU_LOG_SINK=stdout` — default routes non-TTY to journald        |
| `Error::IO` at init in a container             | journald socket unavailable; use `KAMU_LOG_SINK=stdout`                |
| Tests hang at `init()`                         | Use `init_or_skip()` per-test or `InitOptions::idempotent(true)`        |
| OTLP exporter slow                             | `SimpleSpanProcessor` is synchronous; high-volume needs a batch processor |
| `service.name` missing from fmt output         | Only attached to startup event + OTLP Resource; aggregators add it from infra metadata |

## SemVer policy

`1.x.y` — breaking changes only on major bumps. Additive changes ship as
minor releases. Bug fixes ship as patches. The `Error` enum is
`#[non_exhaustive]`; new variants are not breaking.

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only through 1.1.1.

[badge-crates]: https://img.shields.io/crates/v/kamu-logging?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-logging?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-logging?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.94-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-logging
[link-docs]: https://docs.rs/kamu-logging
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/logging
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
