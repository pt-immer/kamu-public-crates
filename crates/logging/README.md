# kamu‑logging

`kamu‑logging` is a small helper crate to configure structured logging for
services built by PT IMMER.  It wraps the [`tracing`](https://docs.rs/tracing)
ecosystem and selects an appropriate backend depending on your target
platform.

## Supported targets

* **Systemd (default)** – When the `systemd` feature is enabled, the crate
  initialises a `tracing` subscriber that forwards logs from the `log` crate,
  parses the `RUST_LOG` environment variable, and emits either coloured
  console output or forwards events to journald when not attached to a TTY.

* **WASM (`wasm32` feature)** – On WebAssembly targets the crate installs
  [`console_error_panic_hook`](https://docs.rs/console_error_panic_hook)
  to improve panic messages and configures the [`wasm‑tracing`](https://github.com/pt-immer/wasm-tracing)
  subscriber.

* **Actix Web (`logging‑actix‑web` feature)** – Exposes a
  `get_actix_web_logger()` function returning an Actix Web middleware
  logger.

## Usage

Add the crate to your `Cargo.toml` and call `kamu_logging::init()` early
in `main`.  At least one of the mutually exclusive `systemd` or
`wasm32` features must be enabled.  The `systemd` feature is enabled by
default.

```toml
[dependencies]
kamu‑logging = "0.1.3"
```

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialise logging.  This will forward any logs emitted via the
    // `log` crate into the `tracing` subscriber and pick the
    // appropriate backend.
    kamu_logging::init()?;

    // Your application logic here.
    Ok(())
}
```

When building for `wasm32` targets, enable the `wasm32` feature and
disable the default features:

```toml
[dependencies]
kamu‑logging = { version = "0.1.3", default‑features = false, features = ["wasm32"] }
```

## License

This project is licensed under the MIT License.  See the [LICENSE](LICENSE)
file for details.
