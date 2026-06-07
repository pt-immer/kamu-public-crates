# `kamu-snap-crypto-actix`

actix-web inbound-verify glue for Bank Indonesia SNAP BI service signatures —
the framework adapter for [`kamu-snap-crypto`](https://crates.io/crates/kamu-snap-crypto).

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
