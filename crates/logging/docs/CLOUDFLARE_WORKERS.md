# Cloudflare Workers

`kamu-logging` supports Rust Cloudflare Workers through its `wasm32` feature.
The implementation follows Cloudflare's Rust guidance: compile for
`wasm32-unknown-unknown`, use `workers-rs` / `worker-build` for the Worker
entrypoint, and send `tracing` events to the JavaScript console so Workers Logs
can collect them.

## Prerequisites

- Rust with the `wasm32-unknown-unknown` target installed.
- Node.js / npm for Wrangler.
- Wrangler and `worker-build` for local development and deployment.

Cloudflare's generated Rust Worker template already wires most of this together.
For this repository, see the standalone example in
`examples/cloudflare-worker/`.

## Cargo setup

Disable default features and enable only `wasm32`:

```toml
[dependencies]
kamu-logging = { version = "1", default-features = false, features = ["wasm32"] }
worker = "0.8"
```

The default `systemd` and `with-actix-web` features are native-only and are
intentionally incompatible with `wasm32`.

## Initialize logging

Workers have a single global tracing subscriber, gated by an internal
`OnceLock` — the **first** `init_with` call wins for the isolate's lifetime,
and later calls are no-ops. When the filter depends on an `Env` binding
(see [Configure filtering from Worker variables](#configure-filtering-from-worker-variables)
below), this matters: install the subscriber on the **first fetch**, where
`Env` is available. Do **not** install from `#[event(start)]` as well — `start`
runs before any fetch, so a start-time install latches the default filter and
the fetch-time install with the env-derived filter is silently discarded.

`init_with` calls `console_error_panic_hook::set_once()` internally on the
wasm32 path, so a separate `#[event(start)]` handler is not required for panic
hook installation.

```rust
use kamu_logging::{Format, InitOptions, Sink, init_with};
use worker::{Context, Env, Request, Response, Result, event};

#[event(fetch)]
async fn main(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let _ = init_with(
        InitOptions::default()
            .with_format(Format::Json)
            .with_sink(Sink::Stdout)
            .idempotent(true),
    );
    let _ = &env;
    Response::ok("ok")
}
```

`Format::Auto` resolves to JSON on wasm32 so Workers Logs can index structured
fields more effectively. `Format::Pretty` falls back to compact, non-ANSI output
because terminal styling is not useful in the Worker console.

## Configure filtering from Worker variables

Rust libraries cannot read Cloudflare `Env` bindings by themselves. Read the
binding inside `#[event(fetch)]` and pass it into
`InitOptions::with_default_filter` on the first call:

```rust
use kamu_logging::{Format, InitOptions, Sink, init_with};
use worker::Env;

fn init_logging(env: &Env) {
    let mut options = InitOptions::default()
        .with_format(Format::Json)
        .with_sink(Sink::Stdout)
        .idempotent(true);

    if let Ok(filter) = env.var("RUST_LOG") {
        options = options.with_default_filter(filter.to_string());
    }

    let _ = init_with(options);
}
```

`RUST_LOG` is a deploy-time variable and is constant for the isolate's
lifetime, so first-fetch-wins is sufficient — no reloadable filter layer is
needed. If you also wire up `#[event(start)]`, restrict it to work that is
independent of the subscriber (for example, application-specific one-time
setup); do **not** call `init_with` from `start` when filtering is driven by
`Env`.

In `wrangler.toml`:

```toml
[vars]
RUST_LOG = "info,my_worker=debug"
```

## Enable Workers Logs

Workers Logs collects invocation logs, custom `console.log` output, errors, and
uncaught exceptions. Enable observability in `wrangler.toml`:

```toml
[observability]
enabled = true
head_sampling_rate = 1
```

For high-volume Workers, lower `head_sampling_rate` to control log volume and
cost.

## Correlation IDs

The header extraction helpers are framework-agnostic and work with
`worker::Request` headers:

```rust
use kamu_logging::correlation::{DEFAULT_HEADER_CHAIN, extract_from_headers};
use worker::Request;

fn correlation_id(req: &Request) -> Option<String> {
    let headers = req.headers();
    extract_from_headers(&headers, DEFAULT_HEADER_CHAIN, |headers, name| {
        headers.get(name).ok().flatten()
    })
}
```

The default chain checks `x-request-id`, `x-correlation-id`, then `traceparent`.
For `traceparent`, the W3C trace-id segment is used as the correlation id.

## Local development

From the example directory:

```bash
npm install
npm run dev
```

Then visit `http://localhost:8787/` and watch Wrangler's console output for
JSON-formatted tracing events.

## Limitations

- `Sink::Journald` is rejected on wasm32 because Workers do not have systemd.
- `with-actix-web` is native-only; Workers use `workers-rs` handlers instead.
- `with-otlp` is native-only in this release. Use Workers Logs, Tail Workers,
  or Logpush for production log export.
- The library does not depend on the `worker` crate. Worker APIs stay in your
  application and in the example project.
