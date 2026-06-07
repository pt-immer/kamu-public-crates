# `kamu-snap-crypto-axum`

axum / `http` inbound-verify glue for Bank Indonesia SNAP BI service signatures —
the framework adapter for [`kamu-snap-crypto`](https://crates.io/crates/kamu-snap-crypto).

## What this crate is

A single function, [`verify_request`], operating on `http::request::Parts` + body
bytes. `Parts` gives clean access to method/headers without consuming the body, so
consumers extract the body via `axum::body::Bytes` (or `axum::body::to_bytes`) and
then call this function inside an extractor / handler. The crate depends only on
`http` and `kamu-snap-crypto` — no axum/tower dependency.

> A full `tower::Layer` wrapper is intentionally deferred to a future release;
> layered body extraction needs careful buffer-and-replay best designed against a
> production caller.

## Quickstart

```rust,no_run
use axum::{body::Bytes, extract::Request};
use kamu_snap_crypto_axum::verify_request;

async fn check(req: Request) -> Result<(), kamu_snap_crypto::Error> {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap_or_default();
    verify_request(&parts, &bytes, "client-secret")
}
```

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only in `pt-immer/lib-snap`.
