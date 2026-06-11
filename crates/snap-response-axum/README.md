# `kamu-snap-response-axum`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

`axum::response::IntoResponse` adapter for
[`kamu-snap-response`](https://crates.io/crates/kamu-snap-response).

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

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

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-response-axum?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-response-axum?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-response-axum?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.88-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-snap-response-axum
[link-docs]: https://docs.rs/kamu-snap-response-axum
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-response-axum
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
