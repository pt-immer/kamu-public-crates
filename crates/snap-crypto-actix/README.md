# `kamu-snap-crypto-actix`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]

[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

actix-web inbound-verify glue for Bank Indonesia SNAP BI service signatures —
the framework adapter for [`kamu-snap-crypto`](https://crates.io/crates/kamu-snap-crypto).

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates) workspace.

## What this crate is

A single function, [`verify_request`], that takes the parts of an `actix-web`
request (method, path, headers, body) plus a client secret and returns `Ok(())`
iff the incoming `X-SIGNATURE` validates against the canonical SNAP BI service
`stringToSign`. All crypto lives in `kamu-snap-crypto`; this crate only bridges
actix's `Method` / `HeaderMap` types.

> A full `Transform`/middleware wrapper is intentionally deferred to a future
> release — body extraction inside actix middleware needs buffer-and-replay
> plumbing best designed against a production caller. For now, call
> `verify_request` from inside your handler (or a custom `FromRequest`
> extractor) once the body is materialised.

## Quickstart

```rust,no_run
use actix_web::{HttpRequest, web};
use kamu_snap_crypto_actix::verify_request;

async fn handler(req: HttpRequest, body: web::Bytes) -> actix_web::HttpResponse {
    match verify_request(req.method(), req.path(), req.headers(), &body, "client-secret") {
        Ok(()) => actix_web::HttpResponse::Ok().finish(),
        Err(_) => actix_web::HttpResponse::Unauthorized().finish(),
    }
}
```

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only in `pt-immer/lib-snap`.

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-crypto-actix?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-crypto-actix?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-crypto-actix?style=flat-square
[badge-msrv]: https://img.shields.io/badge/MSRV-1.88-blue?style=flat-square&logo=rust

[link-crates]: https://crates.io/crates/kamu-snap-crypto-actix
[link-docs]: https://docs.rs/kamu-snap-crypto-actix
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-crypto-actix
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
