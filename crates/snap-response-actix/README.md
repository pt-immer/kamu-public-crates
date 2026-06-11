# `kamu-snap-response-actix`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

`actix_web::Responder` adapter for
[`kamu-snap-response`](https://crates.io/crates/kamu-snap-response).

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

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

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-response-actix?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-response-actix?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-response-actix?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.88-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-snap-response-actix
[link-docs]: https://docs.rs/kamu-snap-response-actix
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-response-actix
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
