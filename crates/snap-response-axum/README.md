# `kamu-snap-response-axum`

`axum::response::IntoResponse` adapter for
[`kamu-snap-response`](https://crates.io/crates/kamu-snap-response).

## What this crate is

Enables returning a `SnapResponse<T>` directly from an axum 0.7+ handler. The
orphan-rule shim is the `AxumResponder<T>` newtype plus a `.into_axum()` extension
method. Like the actix adapter, it falls back to `500 INTERNAL_SERVER_ERROR` when
the `responseCode` cannot be parsed back into an HTTP status — no `.unwrap()` on
the response path.

## Quickstart

```rust,no_run
use kamu_snap_response::{ServiceCode, SnapResponse};
use kamu_snap_response_axum::SnapResponderExt;

async fn handler() -> impl axum::response::IntoResponse {
    let svc = ServiceCode::new(11).unwrap();
    SnapResponse::ok("payload", svc, 0).into_axum()
}
```

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only in `pt-immer/lib-snap`.
