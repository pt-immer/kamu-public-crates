//! `kamu-logging` — opinionated `tracing` setup for PT IMMER services.
//!
//! Call [`init`] from `main` for the zero-config path, or [`init_with`] with
//! an [`InitOptions`] builder for explicit format / sink / filter / OTLP
//! configuration. See the crate README for worked examples.
//! On `wasm32-unknown-unknown`, enable only the `wasm32` feature to install a
//! panic hook and emit `tracing` events to the JavaScript console. This path is
//! suitable for Cloudflare Workers via `workers-rs`; systemd, Actix Web, and
//! OTLP exporter features are native-only.
//!
//! Re-exports common `tracing` items so consumers can avoid a separate
//! `tracing` import for the basic logging vocabulary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(all(feature = "systemd", feature = "wasm32"))]
compile_error!("Feature \"systemd\" can't be combined with \"wasm32\".");

#[cfg(all(feature = "with-actix-web", feature = "wasm32"))]
compile_error!("Feature \"with-actix-web\" can't be combined with \"wasm32\".");

#[cfg(all(feature = "with-otlp", feature = "wasm32"))]
compile_error!("Feature \"with-otlp\" can't be combined with \"wasm32\".");

#[cfg(not(any(feature = "systemd", feature = "wasm32")))]
compile_error!("At least feature \"systemd\" or \"wasm32\" must be enabled.");

pub mod correlation;

mod init;
mod options;

#[cfg(feature = "with-actix-web")]
mod actix;

#[cfg(feature = "with-otlp")]
pub mod otlp;

pub use crate::init::{init, init_or_skip, init_with};
pub use crate::options::{Format, InitOptions, ParseFormatError, ParseSinkError, Sink};

#[cfg(feature = "with-actix-web")]
pub use crate::actix::{EnrichedRootSpanBuilder, get_actix_web_logger, get_actix_web_logger_with};

#[cfg(feature = "with-otlp")]
pub use crate::otlp::{SpanProcessorMode, flush_otlp, shutdown_otlp};

/// Re-exports of the common `tracing` vocabulary so consumers can
/// `use kamu_logging::{info, instrument, ...}` without a separate import.
pub use tracing::{Level, Span, debug, enabled, error, event, info, instrument, span, trace, warn};

/// Errors returned by [`init`] / [`init_with`].
///
/// Marked `#[non_exhaustive]` so future variants are not breaking changes.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// I/O failure during subscriber setup (typically the journald socket).
    #[error("{0}")]
    IO(#[from] std::io::Error),

    /// This crate already installed the subscriber and `idempotent` was false.
    #[error("logging subscriber already initialized")]
    AlreadyInitialized,

    /// The requested options are not supported on the selected target.
    #[error("invalid logging configuration: {0}")]
    InvalidConfiguration(String),

    /// An environment variable contains an unsupported or malformed value.
    #[error("invalid {variable}: expected {expected}")]
    InvalidEnvironmentValue {
        /// Name of the invalid environment variable.
        variable: String,
        /// Accepted grammar, without echoing the rejected value.
        expected: &'static str,
    },

    /// Another crate installed the process-global tracing subscriber.
    #[error("a foreign tracing subscriber already owns the process-global slot")]
    ForeignGlobalSubscriber,

    /// Another crate installed the process-global `log` facade.
    ///
    /// The tracing subscriber and any OTLP provider are already committed when
    /// this error is returned; only the `log`-to-`tracing` bridge is foreign.
    #[cfg(feature = "systemd")]
    #[error("a foreign logger already owns the process-global log facade")]
    ForeignGlobalLogger,

    /// A prior installation panicked after claiming the tracing subscriber.
    #[cfg(feature = "systemd")]
    #[error("logging installation stopped before the log bridge committed")]
    InstallationIncomplete,

    /// OTLP exporter construction failed.
    #[cfg(feature = "with-otlp")]
    #[error("OTLP init failed: {0}")]
    OtlpInit(String),
}
