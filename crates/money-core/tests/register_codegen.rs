//! Runs the build script's own fixture tests.
//!
//! `build/iso4217.rs` parses and validates the vendored ISO 4217 register, and carries a
//! `#[cfg(test)]` module covering both — the digest of the vendored file, the edition gate's
//! accept and refuse paths, and the agreement between `VENDORED.md`'s credit table and the
//! machine-checked `edition` manifest.
//!
//! **A build script is not a test target**, so none of that would ever run: cargo compiles
//! `build.rs` to produce the table and never builds it with `--test`. Pulling the module in
//! here does. An integration test is its own crate, so `cfg(test)` holds throughout it, and the
//! module's `#[cfg(test)] mod tests` compiles and runs as `iso4217::tests::*`.
//!
//! The alternative was to drop those fixtures and rely on the generated table's own invariants
//! in `src/iso.rs`. Those cover what the register IS; these cover what happens when it is
//! wrong, which is the property the crate actually argues from — a replaced or edited register
//! must fail the build rather than settle amounts differently.

#![allow(missing_docs)]

#[path = "../build/iso4217.rs"]
mod iso4217;

/// Drive the emitter end to end.
///
/// This started as a way to stop `dead_code` firing — in a test binary nothing calls
/// `generate`, because the fixtures below it exercise the parser and the validator instead.
/// Silencing the lint would have been a line shorter and proved nothing; calling the function
/// proves the whole pipeline runs and emits the register, which no other test here does.
///
/// The assertions are deliberately shallow. What the table CONTAINS is pinned by
/// `src/iso.rs`'s own tests against the compiled result, which is a stronger check than
/// grepping token text — this only has to catch an emitter that produces nothing at all, the
/// failure that would otherwise surface as hundreds of unresolved names in the consumer.
#[test]
fn the_emitter_produces_the_register() {
    let tokens = iso4217::generate().to_string();
    assert!(tokens.contains("pub enum Iso4217"), "the emitted register declares no Iso4217 enum");
    assert!(
        tokens.contains("USD") && tokens.contains("IDR"),
        "the emitted register is missing currencies it must contain"
    );
    assert!(
        tokens.len() > 10_000,
        "the emitted register is {} bytes, which is far too small for 178 currencies plus \
         their lookups and marker types",
        tokens.len()
    );
}
