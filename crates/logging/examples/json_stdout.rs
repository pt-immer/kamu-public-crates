//! JSON-on-stdout example for log aggregators (Vector, Promtail, Datadog).
//!
//! Run with `cargo run --example json_stdout | jq .`. Pipe-friendly:
//! every event is one line of JSON.

use kamu_logging::{Format, InitOptions, Sink, info, init_with};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_with(
        InitOptions::default()
            .with_service_name("json-stdout-example")
            .with_format(Format::Json)
            .with_sink(Sink::Stdout),
    )?;
    info!(user_id = 12345, "user signed in");
    info!(order_id = "ORD-789", total_cents = 19_900, "order placed");
    Ok(())
}
