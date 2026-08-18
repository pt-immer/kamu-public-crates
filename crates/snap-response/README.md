# `kamu-snap-response`

[![Crates.io][badge-crates]][link-crates]
[![docs.rs][badge-docs]][link-docs]
[![CI][badge-ci]][link-ci]
[![License][badge-license]][link-license]
[![MSRV][badge-msrv]][link-msrv]

Valid-by-construction responses for Bank Indonesia SNAP BI.

Part of the [`kamu-public-crates`](https://github.com/pt-immer/kamu-public-crates)
workspace.

## Response states

| State | Meaning | Framework status |
| --- | --- | --- |
| `Success` | Valid 2xx code and typed payload | Code's status |
| `Failure` | Valid non-2xx code and optional `ErrorClass` | Code's status |
| `Malformed` | Invalid upstream code preserved verbatim | Always 500 |

All three states serialize to SNAP BI's flat JSON object. Private state fields
prevent local construction of success-without-payload or failure-with-payload.

```text
responseCode = HHH SS CC
               │   │  └─ case code (00–99)
               │   └──── service code (00–99)
               └──────── HTTP status
```

## Server

```rust
use kamu_snap_response::{Error, ServiceCode, SnapResponse};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Balance {
    account_no: String,
    current_balance: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = ServiceCode::try_from(11)?;

    let success = SnapResponse::success(
        Balance {
            account_no: "1234".into(),
            current_balance: "99000.00".into(),
        },
        service,
    )?;
    assert_eq!(success.response_code(), "2001100");

    let failure: SnapResponse<Balance> =
        SnapResponse::failure(Error::InsufficientFunds, service);
    assert_eq!(failure.response_code(), "4031114");

    Ok(())
}
```

`SnapResponse::success` accepts JSON objects and unit. It rejects scalar,
sequence, `responseCode`, and `responseMessage` payloads before a response is
constructed.

## Client

```rust
use kamu_snap_response::SnapResponse;
use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    value: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wire = r#"{
        "responseCode": "2001100",
        "responseMessage": "Successful",
        "value": "ready"
    }"#;
    let response: SnapResponse<Payload> = serde_json::from_str(wire)?;

    match response {
        SnapResponse::Success(success) => assert_eq!(success.payload().value, "ready"),
        SnapResponse::Failure(failure) => {
            eprintln!("{}: {}", failure.response_code(), failure.response_message());
        }
        SnapResponse::Malformed(malformed) => {
            eprintln!("malformed code: {}", malformed.response_code());
        }
        _ => {}
    }

    Ok(())
}
```

`ErrorClass` classifies all 61 standard error pairs without fabricating context.
Unknown but syntactically valid codes remain `Failure` with no known class.

## Response-code layers

| Type | Contract |
| --- | --- |
| `ValidResponseCode` | All seven ASCII digits and HTTP status validated |
| `RawResponseCode` | Verbatim malformed upstream value |
| `ResponseCode` | Total parser containing one of the above |
| `ServiceCode` / `CaseCode` | Two-digit validated components |

## Crypto feature

The optional `crypto` feature converts `kamu-snap-crypto` errors while retaining
their operational class:

- authentication → 401;
- invalid request → 400;
- local key/configuration or unknown internal failure → 500.

Wire messages never include upstream key-parser or decoder details. The source
remains available through the Rust error chain for server-side diagnostics.

## Framework adapters

- [`kamu-snap-response-actix`](../snap-response-actix)
- [`kamu-snap-response-axum`](../snap-response-axum)

Both adapters serialize before applying the response status and return HTTP 500
if serialization fails.

## Migrating from 2.x

| 2.x | 3.x |
| --- | --- |
| `SnapResponse::ok(payload, service, case)` | `SnapResponse::success(payload, service)?` or `success_with_case` |
| `SnapResponse::err(error, service)` | `SnapResponse::failure(error, service)` |
| `response.envelope.response_code` | `response.response_code()` |
| `response.payload: Option<T>` | `response.payload()` or match `Success` |
| `ResponseCode::classify() -> Option<Error>` | `Option<ErrorClass>` |
| numeric panic-based constructors | `CaseCode` and fallible numeric constructors |

## Examples

[`client_parse`](examples/client_parse.rs) parses success, failure, and malformed
SNAP BI responses. Run it with `cargo run --example client_parse`.

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option (`MIT OR Apache-2.0`).

[badge-crates]: https://img.shields.io/crates/v/kamu-snap-response?style=flat-square&logo=rust
[badge-docs]: https://img.shields.io/docsrs/kamu-snap-response?style=flat-square&logo=docs.rs&label=docs.rs
[badge-ci]: https://img.shields.io/github/actions/workflow/status/pt-immer/kamu-public-crates/on-main-pushed.yml?branch=main&style=flat-square&label=CI
[badge-license]: https://img.shields.io/crates/l/kamu-snap-response?style=flat-square
[badge-msrv]: https://img.shields.io/crates/msrv/kamu-snap-response?style=flat-square&logo=rust&label=MSRV

[link-crates]: https://crates.io/crates/kamu-snap-response
[link-docs]: https://docs.rs/kamu-snap-response
[link-ci]: https://github.com/pt-immer/kamu-public-crates/actions/workflows/on-main-pushed.yml
[link-license]: https://github.com/pt-immer/kamu-public-crates/blob/main/crates/snap-response
[link-msrv]: https://github.com/pt-immer/kamu-public-crates/blob/main/Cargo.toml
