# kamu-logging

[![CI](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml/badge.svg)](https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-release-published.yml)
[![crates.io](https://img.shields.io/crates/v/kamu-logging.svg)](https://crates.io/crates/kamu-logging)
[![docs.rs](https://img.shields.io/docsrs/kamu-logging)](https://docs.rs/kamu-logging)

`kamu-logging` is a small helper crate to configure structured logging for
services built by PT IMMER. It wraps the [`tracing`](https://docs.rs/tracing)
ecosystem and selects an appropriate backend depending on your target platform.

It is part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

## Supported targets

- **Systemd (default)** — When the `systemd` feature is enabled, the crate
  initializes a `tracing` subscriber that forwards logs from the `log` crate,
  parses the `RUST_LOG` environment variable, and emits either colored console
  output or forwards events to journald when not attached to a TTY.

- **WASM (`wasm32` feature)** — On WebAssembly targets the crate installs
  [`console_error_panic_hook`](https://docs.rs/console_error_panic_hook) to
  improve panic messages and configures the
  [`wasm-tracing`](https://github.com/dsgallups/wasm-tracing) subscriber.

- **Actix Web (`with-actix-web` feature)** — Exposes a `get_actix_web_logger()`
  function returning an Actix Web middleware logger.

The `systemd` and `wasm32` features are mutually exclusive; at least one must be
enabled. The `systemd` feature is enabled by default.

## Usage

Add the crate to your `Cargo.toml` and call `kamu_logging::init()` early in
`main`:

```toml
[dependencies]
kamu-logging = "0.1"
```

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging: forwards `log` records into the `tracing`
    // subscriber and picks the appropriate backend for the target.
    kamu_logging::init()?;

    // Your application logic here.
    Ok(())
}
```

When building for `wasm32` targets, enable the `wasm32` feature and disable the
default features:

```toml
[dependencies]
kamu-logging = { version = "0.1", default-features = false, features = ["wasm32"] }
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
