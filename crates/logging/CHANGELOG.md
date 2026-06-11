# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project follows
[SemVer](https://semver.org/) from `1.0.0` onwards.

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
