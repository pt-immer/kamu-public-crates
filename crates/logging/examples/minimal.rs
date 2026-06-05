//! Minimal example: zero-config init.
//!
//! Run with `cargo run --example minimal`. Set `RUST_LOG=debug` to see the
//! debug event.

use kamu_logging::{debug, info, init};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init()?;
    info!("hello from kamu-logging");
    debug!(value = 42, "structured field example");
    Ok(())
}
