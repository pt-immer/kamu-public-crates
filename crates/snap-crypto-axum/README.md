# `kamu-snap-crypto-axum`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

axum / `http` inbound-verify glue for Bank Indonesia SNAP BI service signatures —
the framework adapter for [`kamu-snap-crypto`](https://crates.io/crates/kamu-snap-crypto).

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

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

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-crypto-axum?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-crypto-axum?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-crypto-axum?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.85-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-snap-crypto-axum
[link-docs]: https://docs.rs/kamu-snap-crypto-axum
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-crypto-axum
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
