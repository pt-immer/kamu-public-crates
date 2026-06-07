# `kamu-snap-response`

Framework-agnostic response envelope + error taxonomy for Bank Indonesia SNAP BI.

## What this crate is

| Item | Purpose |
|---|---|
| `SnapResponse<T>` | Typed envelope + optional typed payload (flat JSON wire shape) |
| `SnapEnvelope` | `responseCode` + `responseMessage` |
| `Error` | 61-variant SNAP BI taxonomy + feature-gated `Crypto` bridge |
| `Category` | `Success` / `Message` / `Business` / `System` |
| `ResponseCode` | Defensive parser + inverse classifier |
| `ServiceCode` | Validated `0..=99` newtype |

## The `responseCode` formula

```
7 digits: HHH SS CC
         │   │  │
         │   │  └── case code (00..=99) — per-variant per-spec
         │   └───── service code (00..=99) — per-endpoint
         └───────── HTTP status (200, 401, 403, 404, 405, 409, 429, 500, 504)
```

`SnapResponse::ok(payload, ServiceCode::new(11).unwrap(), 0)` →
`responseCode = "2001100"`.

`Error::Unauthorized("...".into()).response_code(ServiceCode::new(11).unwrap())` →
`ResponseCode("4011100")`.

## 30-second quickstart — server side

```rust
use kamu_snap_response::{Error, ServiceCode, SnapResponse};

let svc = ServiceCode::new(11).unwrap();

// Success
let ok: SnapResponse<MyData> = SnapResponse::ok(my_data, svc, 0);

// Error
let err: SnapResponse<MyData> = SnapResponse::err(Error::InsufficientFunds, svc);
```

## 30-second quickstart — client side

```rust
use kamu_snap_response::SnapResponse;

let wire: &str = "...";
let resp: SnapResponse<MyData> = serde_json::from_str(wire)?;

// Defensive parsing: malformed responseCode preserves raw()
println!("{}", resp.envelope.response_code.raw());

// Classify back to a typed Error variant for retry policy
if let Some(err) = resp.envelope.response_code.classify() {
    use kamu_snap_response::Category::*;
    match err.category() {
        Business => println!("do not retry: {err}"),
        System => println!("retry with backoff: {err}"),
        _ => {}
    }
}
# Ok::<(), serde_json::Error>(())
```

Full example: `examples/client_parse.rs`.

## Defensive parsing

`ResponseCode::parse` is **total** — never errors. Malformed wire codes preserve `raw()` while `http()` / `service()` / `case()` return `None`:

```rust
use kamu_snap_response::ResponseCode;

let code = ResponseCode::parse("500000"); // BRI source-doc 6-digit defect
assert_eq!(code.raw(), "500000");
assert_eq!(code.http(), None);
```

This closes the prior bug where deserialisation hard-failed on a malformed code, losing the entire response.

## Inverse classifier

`ResponseCode::classify()` maps received wire codes back to typed `Error` variants:

```rust
use kamu_snap_response::{Error, ResponseCode};

let code = ResponseCode::parse("4011100");
assert!(matches!(code.classify(), Some(Error::Unauthorized(_))));
```

String-bearing variants (e.g. `Unauthorized(String)`) reconstruct with an empty string — the wire `responseMessage` is still available via `envelope.response_message` for display.

## `#[non_exhaustive]`

Both `Error` and `Category` are `#[non_exhaustive]`. Future SNAP BI variants can add cases without a breaking change.

## Feature flags

| Feature | Default | Purpose |
|---|---|---|
| `crypto` | off | Enables `Error::Crypto(#[from] kamu_snap_crypto::Error)` |

Enable when server handlers want to propagate crypto failures into the SNAP error taxonomy.

## Framework adapters

`Responder` (actix-web) and `IntoResponse` (axum) impls live in:

- `kamu-snap-response-actix`
- `kamu-snap-response-axum`

Both are thin and defensive — no `.unwrap()` on the response path.

## Migration from v1.x

| v1.x | v2.0 |
|---|---|
| `SNAPResponse<T>` | `SnapResponse<T>` |
| `SNAPResponseCommon` | `SnapEnvelope` |
| `ResponseError` / `Error` aliases | `Error` (one canonical name) |
| `Error::Unathorized(s)` | `Error::Unauthorized(s)` (typo fixed) |
| `error.get_http_status_code()` | `error.http_status()` |
| `error.get_code(svc)` | `error.response_code(ServiceCode::new(svc).unwrap())` |
| `Error::default()` → `GeneralError` | removed — construct explicitly |
| `traced_guard!` macro | removed — use `?` + `Error::Crypto(#[from])` |
| Custom Deserialize silently swallowed payload errors | propagates errors loudly |

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`). Previously MIT-only in `pt-immer/lib-snap`.
