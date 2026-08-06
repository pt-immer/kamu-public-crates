# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows
[SemVer](https://semver.org/) from `1.0.0` onwards.

## [2.1.0] — 2026-08-07

### Added

- A `correlation` feature carrying `TraceParent`, `extract_from_headers`,
  `parse_traceparent_trace_id`, `with_id`, and `span`. It declares no optional
  dependencies, so enabling it alone builds the crate with no subscriber, sink,
  or exporter in the dependency graph:

  ```toml
  kamu-logging = { version = "2", default-features = false, features = ["correlation"] }
  ```

- `systemd`, `wasm32`, and `with-actix-web` now imply `correlation`, and any one
  of the four satisfies the crate's feature guard. Existing feature selections
  are unaffected. `features = ["with-actix-web"]` alone now builds the Actix Web
  middleware without a subscriber.

### Changed

- `init`, `init_or_skip`, `init_with`, `InitOptions`, `Format`, `Sink`,
  `ParseFormatError`, `ParseSinkError`, and `Error` now require `systemd` or
  `wasm32`. Every previously valid feature combination already enabled one.
- `with-otlp` without `systemd` is rejected by a named `compile_error!` instead
  of failing on a missing subscriber.

## [2.0.0] — 2026-07-28

Make process-global ownership explicit and reject malformed data at the
boundary.

### Added

- `TraceParent`, a parsed four-field W3C Trace Context value with typed access
  to version, trace ID, parent ID, flags, and the sampled bit.
- `TraceParentError`, `ParseFormatError`, and `ParseSinkError`.
- `Error::ForeignGlobalSubscriber` and `Error::ForeignGlobalLogger`, separating
  ownership conflicts from this crate's own duplicate initialization.
- `Error::InstallationIncomplete` for a panic between subscriber and logger
  commits.
- A 128-byte visible-ASCII policy for untrusted request and correlation IDs.
- End-to-end Actix tests covering header precedence, repeated and non-ASCII
  values, malformed `traceparent`, and successful and failed request
  completion.
- `#![forbid(unsafe_code)]`.

### Changed

- **Breaking:** `Format::from_env_value` and `Sink::from_env_value` now return
  `Result` and reject unknown values. `KAMU_LOG_FORMAT`, `KAMU_LOG_SINK`, and
  the selected filter environment variable now fail clearly when malformed
  instead of silently falling back.
- **Breaking:** `Sink::Auto` now selects stderr on native targets. Journald is
  an explicit opt-in. TTY detection follows the selected stdout or stderr
  stream.
- **Breaking:** `Error::TracingGlobal` and `Error::TracingLog` were replaced by
  the two ownership variants above.
- Initialization now serializes concurrent callers and records subscriber,
  logger-bridge, and OTLP-provider ownership in one committed state.
- A foreign `log` owner is reported after the tracing subscriber and optional
  OTLP provider commit; those remain active, and retries preserve the conflict.
- `init_or_skip` and `idempotent(true)` succeed only for a complete subscriber
  installed by this crate. They never suppress a foreign global owner.
- `OtlpConfig` and `InitOptions` use redacted `Debug` implementations. HTTP
  header values, endpoint text, and secret-like resource attributes are not
  printed.
- `OtlpConfig::new` treats its endpoint as a collector base URL and appends the
  standard `/v1/traces` path unless already present.
- `InitOptions::with_service_name` supplies the OTLP resource service name when
  `OtlpConfig` does not override it.

### Fixed

- Validate the complete W3C `traceparent` base format: lowercase hexadecimal,
  nonzero trace and parent IDs, two-digit flags, exact version-`00` length, and
  future-version suffix delimiters.
- Keep a newly built OTLP provider local until the subscriber commits. A
  foreign subscriber now shuts that provider down instead of poisoning later
  `flush_otlp` and `shutdown_otlp` calls.
- Require the OTLP runtime test to receive a `POST /v1/traces` request with
  `application/x-protobuf` and a nonempty body after a successful loopback
  bind.
- Correct service-name and wasm32 repeated-init documentation.

## [1.6.0] — 2026-07-27

Toolchain maintenance. No public API changes.

### Changed

- Minimum supported Rust version raised to 1.94.
- Unit tests and the Cloudflare Workers guide now spell loopback as
  `127.0.0.1` rather than `localhost`, matching the rest of the fleet.
  `localhost` resolves `::1` first, which is the wrong answer wherever a
  service is published on IPv4 only.

## [1.5.0] — 2026-06-15

Move OTLP span export off the request path so it no longer blocks the thread
that closes a span.

### Added

- The OTLP exporter now uses an in-process `BatchSpanProcessor` that flushes
  from a dedicated background OS thread — export no longer runs inline on the
  calling thread, and **no async runtime is required**.
- `SpanProcessorMode` (`Batch` / `Simple`) and `OtlpConfig::with_processor` to
  pick the export strategy; `Batch` is the default.
- Batch tuning on `OtlpConfig`: `with_max_queue_size`, `with_scheduled_delay`,
  and `with_max_export_batch_size` (each falls back to the SDK default /
  `OTEL_BSP_*` env var when unset; ignored under `SpanProcessorMode::Simple`).
- `flush_otlp()` and `shutdown_otlp()` to drain the batch buffer — call
  `shutdown_otlp()` before process exit (e.g. from a `SIGTERM` handler) so the
  final batch is not lost. Both are no-ops when OTLP is not configured, and
  `shutdown_otlp()` is idempotent.
- First runtime test of the OTLP path: it initializes the exporter inside an
  actix/tokio runtime (asserting the `reqwest-blocking` client builds without a
  nested-runtime panic) and confirms a span is exported off-thread to a local
  collector after `flush_otlp()`.

### Changed

- OTLP export is now **batched and off-thread by default** (previously a
  synchronous `SimpleSpanProcessor` that exported every span inline). Spans are
  still delivered, but on a ~5 s timer rather than immediately. Short-lived
  processes should call `shutdown_otlp()` before exit, or select
  `SpanProcessorMode::Simple`, to guarantee delivery. No public function
  signatures changed (additive release).

## [1.4.0] — 2026-06-14

Correctness + wasm32 consistency fixes surfaced by a knowledge-graph audit of
the correlation / init bridges.

### Fixed

- `correlation::parse_traceparent_trace_id` now rejects the W3C-invalid
  all-zero (null) trace-id and the reserved version `ff`, and requires a
  two-hex-digit version — instead of echoing back a meaningless
  `00000000000000000000000000000000` correlation id.
- On the `wasm32` build, `init_with` now honors the `idempotent` flag on a
  repeat init: a second init with `idempotent(false)` returns
  `Error::AlreadyInitialized`, matching the systemd path, instead of silently
  returning `Ok` and swallowing the double-init error.

### Changed

- Documented that `extract_from_headers`'s `get` closure must return a single
  header value (the first occurrence when a header repeats); the
  Cloudflare-Worker example now takes the first comma-separated segment so its
  correlation id matches single-value backends like the actix adapter.
- Clarified the `init_with` rustdoc: the `KAMU_LOG_*` / `with_env_var` env
  sources apply on the systemd build only; the `wasm32` build takes its filter
  solely from `default_filter`.

## [1.3.0] — 2026-06-11

Dependency/toolchain release. No library code or public API changes.

### Changed

- MSRV raised from 1.85 to 1.88.
- The workspace `time` dependency is now floored at `0.3.47` (was pinned
  exactly `=0.3.45`), picking up the RUSTSEC-2026-0009 / CVE-2026-25727 fix
  (RFC 2822 stack-exhaustion DoS). The published manifest no longer hard-pins
  consumers of the `wasm32` feature to `time =0.3.45`.

## [1.2.2] — 2026-06-08

Docs only. No library code or public API changes.

### Changed

- README: added a "Part of the `kamu-public-crates` workspace" line (badge block
  unchanged).

## [1.2.1] — 2026-06-06

Docs/metadata only. No library code or public API changes.

### Fixed

- README CI badge pointed at the old standalone repo's `pr.yml` workflow (404);
  now targets `on-pr-synced.yml`.
- README license section said "MIT" and linked a non-existent `LICENSE` file;
  corrected to the dual `MIT OR Apache-2.0` with `LICENSE-MIT` / `LICENSE-APACHE`.

## [1.2.0] — 2026-06-06

Workspace migration release. No library code or public API changes.

### Changed

- Moved the crate into the
  [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates)
  Cargo workspace; dependencies now resolve through `[workspace.dependencies]`.
- Dual-licensed under `MIT OR Apache-2.0` (previously MIT only).
- MSRV pinned to the workspace value of `1.85` (was declared `1.88`).

## [1.1.1] — 2026-05-28

Patch release. No library code or public API changes; example and docs only.

### Fixed

- Cloudflare Worker example (`examples/cloudflare-worker/src/lib.rs`) called
  `init_with` from both `#[event(start)]` and `#[event(fetch)]`. Because the
  `wasm32` init path is `OnceLock`-gated and first-init-wins, the start-time
  call (with `env = None`) latched the default filter and the fetch-time call
  that reads `env.var("RUST_LOG")` was silently discarded — the wrangler.toml
  `RUST_LOG` binding never reached `EnvFilter`. Dropped the `#[event(start)]`
  handler; `init_wasm32` invokes `console_error_panic_hook::set_once()`
  internally, so the panic hook still installs on first fetch.
- `docs/CLOUDFLARE_WORKERS.md` previously presented start-time init and
  per-fetch idempotent init as interchangeable, which is what produced the
  bug. Guidance now mandates first-fetch install when filtering is driven by
  an `Env` binding and warns against calling `init_with` from `start`.

## [1.1.0] — 2026-05-27

Additive feature release focused on first-class Cloudflare Worker support.

### Added

- Cloudflare Worker-compatible `wasm32` logging path using `tracing-web`,
  `tracing-subscriber` JSON/time formatting, and `time` with
  `wasm-bindgen`.
- Dedicated Cloudflare Worker example app in
  `examples/cloudflare-worker/` with `workers-rs`, Wrangler config, and
  observability enabled.
- `docs/CLOUDFLARE_WORKERS.md` guide covering setup, filtering,
  correlation ids, and Workers Logs.
- `tests/options.rs` coverage for env-value parsing and invalid
  configuration error display.
- `just validate-wasm32` and Worker example validation wired into CI.

### Changed

- `wasm32` logging now targets Cloudflare Workers / web console output
  instead of the previous `wasm-tracing` path.
- `Format::Auto` resolves to JSON on wasm32; `Format::Pretty` falls back
  to compact non-ANSI output.
- `init_with()` now rejects unsupported wasm options such as
  `Sink::Journald` with `Error::InvalidConfiguration`.

## [1.0.0] — 2026-05-27

First stable release. Single breaking-change release that turns the crate
from a thin `tracing-subscriber` wrapper into the canonical PT IMMER
logging primitive.

### Added

- `InitOptions` builder + `init_with(opts)` for explicit configuration.
- `Format` enum (`Auto`, `Compact`, `Pretty`, `Json`) — JSON output for
  log aggregators (Vector, Promtail, Datadog, Fluent Bit).
- `Sink` enum (`Auto`, `Stdout`, `Stderr`, `Journald`) — explicit
  sink selection with `Auto` preserving previous TTY-aware behavior.
- Env-var triggers `KAMU_LOG_FORMAT` and `KAMU_LOG_SINK` for zero-code
  adoption.
- `init_or_skip()` shortcut and `InitOptions::idempotent(true)` for test
  harnesses and embedded CLI runs.
- `with_service_name`, `with_default_filter`, `with_env_var` builder
  methods for service tagging and per-binary log env vars.
- `correlation` module: `extract_from_headers`, `parse_traceparent_trace_id`,
  `with_id`, `span` helpers. Default header chain: `X-Request-ID`,
  `X-Correlation-ID`, `traceparent`.
- `EnrichedRootSpanBuilder` for `tracing-actix-web` — adds
  `correlation_id` to the root span automatically.
- `get_actix_web_logger_with::<RSB>()` for custom `RootSpanBuilder`
  implementations.
- `with-otlp` feature (`opentelemetry` 0.32, `opentelemetry-otlp` 0.32,
  `tracing-opentelemetry` 0.33) with `OtlpConfig` builder.
- Wider `tracing` re-exports: `Level`, `Span`, `enabled`, `event`,
  `instrument`, `span`, in addition to existing macros.
- Integration tests (`tests/init_*.rs`, `tests/correlation.rs`) and
  examples (`examples/{minimal,json_stdout,actix}.rs`).
- `CHANGELOG.md` and `rust-version = "1.85"` declared in Cargo.toml.
- `#[non_exhaustive]` on `Error` so future variants are non-breaking.
- `compile_error!` invariant added for `with-otlp` + `wasm32` clash.
- `#[deny(missing_docs)]` on the crate root; all public items documented.

### Changed

- **Breaking**: `init()` now returns `Err(Error::AlreadyInitialized)` on
  a second call instead of `Err(TracingGlobal(_))`. Migration: callers
  matching on `TracingGlobal` for duplicate-init detection should match
  `AlreadyInitialized` instead, or switch to `init_or_skip()`.
- **Breaking**: `get_actix_web_logger()` now returns
  `TracingLogger<EnrichedRootSpanBuilder>` instead of
  `TracingLogger<DefaultRootSpanBuilder>`. Adds a `correlation_id`
  field; spans previously dependent on `DefaultRootSpanBuilder`'s exact
  shape may need updating. To opt out, use
  `get_actix_web_logger_with::<DefaultRootSpanBuilder>()`.
- **Breaking**: `Error` is now `#[non_exhaustive]`. Exhaustive `match`
  statements over `Error` will need a wildcard arm.
- README rewritten for the v1 API — feature matrix, env-var triggers,
  troubleshooting table, SemVer policy.
- Edition stays at `2024`; `rust-version = "1.85"` declared explicitly.

### Preserved (not changed)

- `init()` zero-arg form still works (delegates to
  `init_with(InitOptions::default())`).
- TTY-aware default behavior (`Sink::Auto` + `Format::Auto`) matches
  prior versions.
- `tracing_log::LogTracer` bridge still installed on the systemd path.
- `compile_error!` invariants for `systemd` + `wasm32` and
  `with-actix-web` + `wasm32` remain.
- `wasm32` `OnceLock`-gated idempotence behavior unchanged.

## [0.2.0] — 2026-05

- Use journald as sole non-TTY sink (no stderr fallback). (#4)
- Include structured fields in journald `MESSAGE` output. (#3)

## [0.1.3] — 2026

- Cloudflare Workers compatibility. (#2)
- Initial wasm32 feature.

## [0.1.0] — initial release

- Initial public surface: `init()`, `get_actix_web_logger()`, macro re-exports.
