# `kamu-snap-response-actix`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]
[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Actix Web `Responder` adapter for
[`kamu-snap-response`](https://crates.io/crates/kamu-snap-response).

The orphan-rule newtype `ActixResponder<T>` is available through
`SnapResponderExt::into_actix`. It applies an HTTP status only after JSON
serialization succeeds; malformed codes and serialization failures return 500.

```rust,no_run
use kamu_snap_response::{ServiceCode, SnapResponse};
use kamu_snap_response_actix::SnapResponderExt;
use serde::Serialize;

#[derive(Serialize)]
struct Payload {
    status: &'static str,
}

async fn handler() -> impl actix_web::Responder {
    let service = ServiceCode::try_from(11).expect("11 is a valid service code");
    SnapResponse::success(Payload { status: "ready" }, service)
        .expect("Payload is a flat object")
        .into_actix()
}
```

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`).

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-response-actix?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-response-actix?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-pr-synced.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-response-actix?style=flat-square
[badge-msrv]: https://img.shields.io/crates/msrv/kamu-snap-response-actix?style=flat-square&logo=rust&label=MSRV

[link-crates]: https://crates.io/crates/kamu-snap-response-actix
[link-docs]: https://docs.rs/kamu-snap-response-actix
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-pr-synced.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-response-actix
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
