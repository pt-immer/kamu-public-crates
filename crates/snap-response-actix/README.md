# `kamu-snap-response-actix`

`actix_web::Responder` adapter for
[`kamu-snap-response`](https://crates.io/crates/kamu-snap-response).

## What this crate is

Enables returning a `SnapResponse<T>` directly from an actix-web handler. Because
the orphan rule forbids `impl Responder for SnapResponse<T>` here, the crate
provides the `ActixResponder<T>` newtype and a `.into_actix()` extension method.
The impl is defensive: a malformed `responseCode` that cannot be parsed back into
an HTTP status falls back to `500 INTERNAL_SERVER_ERROR` instead of panicking.

## Quickstart

```rust,no_run
use kamu_snap_response::{ServiceCode, SnapResponse};
use kamu_snap_response_actix::SnapResponderExt;

async fn handler() -> impl actix_web::Responder {
    let svc = ServiceCode::new(11).unwrap();
    SnapResponse::ok("payload", svc, 0).into_actix()
}
```

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only in `pt-immer/lib-snap`.
